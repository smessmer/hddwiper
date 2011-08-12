template<class Product>
inline Producer<Product>::ProducerThread::ProducerThread(Assembly<Product> &assembly, boost::function<Product ()> producerfunction)
	:_assembly(assembly), _producerfunction(producerfunction)
{
}

template<class Product>
void Producer<Product>::ProducerThread::operator()()
{
	try
	{
		while (true)
		{
			_assembly.push(_producerfunction());
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
		throw std::logic_error("Producer already initialized. You can't call run() twice.");

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
