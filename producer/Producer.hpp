#pragma once

#ifndef __PRODUCER_HPP__
#define __PRODUCER_HPP__

#include "Assembly.hpp"

#include <boost/thread.hpp>

/**
 * This class is part of a producer/consumer implementation.
 * An instance of this class implements a Producer.
 * The producer runs a thread that produces products and
 * pushes them to a thread-safe assemly.
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
	 * @param assembly
	 * 		The assembly to push the produced products to
	 * @param producer
	 * 		A function that generates one product. This function is called in another
	 * 		thread, so if you use data structures from the current thread, make them thread safe.
	 * 		WARNING: If this function is a member function of a child class of Producer<T>,
	 * 				 be sure to call stop() in the destructor of the child class.
	 * 				 The destructor of Producer<T> will stop the producing thread anyway,
	 * 				 but in the mean time (while waiting for the producing thread to get interrupted),
	 * 				 the producing thread could call this callback function!
	 */
	Producer(Assembly<Product>* assembly, std::function<Product ()> producer);

	/**
	 * Destructor
	 */
	virtual ~Producer();

protected:
	/**
	 * Create a new producer but don't run it.
	 * The producer must be run later by a call to run().
	 * This constructor is useful, if you derive a class
	 * from this producer and use a class attribute as
	 * producer function. In this case, the producer function
	 * isn't initialized when calling the Producer constructor.
	 * So run() must be called in the child class constructor
	 * after its attributes were initialized.
	 * @param assembly
	 * 		The assembly to push the products to
	 */
	Producer(Assembly<Product>* assembly);
	/**
	 * Run the producer. This function can only be called once, and only
	 * if the producer wasn't run in the constructor. See the description
	 * for the constructor Producer(buffersize) for details.
	 * @param producer
	 * 		A function that generates one product. This function is called in another
	 * 		thread, so if you use data structures from the current thread, make them thread safe.
	 * 		WARNING: If this function is a member function of a child class of Producer<T>,
	 * 				 be sure to call stop() in the destructor of the child class.
	 * 				 The destructor of Producer<T> will stop the producing thread anyway,
	 * 				 but in the mean time (while waiting for the producing thread to get interrupted),
	 * 				 the producing thread could call this callback function!
	 */
	void run(std::function<Product ()> producer);

	/**
	 * Stop the producer process, if running, and block until it is terminated.
	 * When this function returns, the producer process is guaranteed to be stopped.
	 */
	void stop();

private:
	class ProducerThread
	{
	public:
		ProducerThread(Assembly<Product>* assembly, std::function<Product ()> producerfunction);

		void operator()();
	private:
		Assembly<Product>* _assembly;
		std::function<Product ()> _producerfunction;
	};

	Assembly<Product>* _assembly;
	boost::thread _producer;
	bool _initialized;
};

#include "impl/Producer.impl.hpp"

#endif /* PRODUCER_HPP_ */
