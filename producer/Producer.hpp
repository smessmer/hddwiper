#pragma once

#ifndef __PRODUCER_HPP__
#define __PRODUCER_HPP__

#include "Assembly.hpp"

#include <boost/thread.hpp>

/**
 * This class is part of a producer/consumer implementation.
 * An instance of this class implements a Producer.
 * The producer has its own assembly (= a queue that contains
 * the already produced products) and runs another
 * thread which fills the assembly.
 * A call to get() blocks until at least one product
 * is available and then returns it.
 *
 * @author Sebastian Meßmer
 */
template<class Product>
class Producer
{
public:
	/**
	 * Create a new producer and run it.
	 *
	 * @param buffersize
	 * 		The number of elements the assembly can store.
	 * 		If the assembly is full, the producer thread is blocked,
	 * 		until a product was fetched from the assembly via a call to get().
	 * @param producer
	 * 		A function that generates one product. This function is called in another
	 * 		thread, so if you use data structures from the current thread, make them thread safe.
	 */
	Producer(const unsigned int buffersize, boost::function<Product ()> producer);

	Producer(const unsigned int buffersize);
	void run(boost::function<Product ()> producer);

	/**
	 * Destructor
	 */
	virtual ~Producer();

	/**
	 * Get the next product from the assembly and free its space,
	 * so the producer can produce another one.
	 *
	 * @return The next product from the assembly
	 */
	const Product get();

	/**
	 * Return the number of products stored in the assembly.
	 *
	 * @return The number of products stored in the assembly
	 */
	unsigned int available_count() const;

private:
	class ProducerThread
	{
	public:
		ProducerThread(Assembly<Product> &assembly, boost::function<Product ()> producer);

		void operator()();
	private:
		Assembly<Product> &_assembly;
		boost::function<Product ()> _producer;
	};
	Assembly<Product> _products;

	boost::thread _producer;
	bool _initialized;
};

template<class Product>
inline Producer<Product>::ProducerThread::ProducerThread(Assembly<Product> &assembly, boost::function<Product ()> producer)
	:_assembly(assembly), _producer(producer)
{
}

template<class Product>
void Producer<Product>::ProducerThread::operator()()
{
	try
	{
		while (true)
		{
			_assembly.push(_producer());
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}

template<class Product>
inline Producer<Product>::Producer(
		const unsigned int buffersize, boost::function<Product ()> producer) :
	_products(buffersize), _producer(ProducerThread(_products,producer)),_initialized(true)
{
}

template<class Product>
inline Producer<Product>::Producer(
		const unsigned int buffersize) :
	_products(buffersize), _producer(),_initialized(false)
{
}

#include <iostream>
template<class Product>
inline void Producer<Product>::run(boost::function<Product ()> producer)
{
	if(_initialized)
	{
		//TODO Exception instead of cerr
		std::cerr << "Producer already initialized"<<std::endl;
		return;
	}
	_producer=boost::thread(ProducerThread(_products,producer));
	_initialized=true;
}

template<class Product>
inline Producer<Product>::~Producer()
{
	_producer.interrupt();
	_producer.join();
}

template<class Product>
inline const Product Producer<Product>::get()
{
	return _products.pop();
}

template<class Product>
inline unsigned int Producer<Product>::available_count() const
{
	return _products.size();
}

#endif /* PRODUCER_HPP_ */
