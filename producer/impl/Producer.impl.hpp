#include <mutex>

template<class Product>
inline Producer<Product>::ProducerThread::ProducerThread(Assembly<Product>* assembly, std::function<Product ()> producerfunction)
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
			_assembly->push(_producerfunction());
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}

template<class Product>
inline Producer<Product>::Producer(
		Assembly<Product>* assembly, std::function<Product ()> producer) :
	_assembly(assembly), _producer(ProducerThread(_assembly,producer)),_initialized(true)
{
}

template<class Product>
inline Producer<Product>::Producer(Assembly<Product>* assembly) :
	_assembly(assembly), _producer(),_initialized(false)
{
}

template<class Product>
inline void Producer<Product>::run(std::function<Product ()> producer)
{
	if(_initialized)
		throw std::logic_error("Producer already initialized. You can't call run() twice.");

	_producer=boost::thread(ProducerThread(_assembly,producer));
	_initialized=true;
}

template<class Product>
inline Producer<Product>::~Producer()
{
	stop();
}

template<class Product>
inline void Producer<Product>::stop()
{
	_producer.interrupt();
	_producer.join();
}
