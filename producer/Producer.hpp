#pragma once

#ifndef __PRODUCER_HPP__
#define __PRODUCER_HPP__

#include "Assembly.hpp"

#include <boost/thread.hpp>

template<class Product>
class Producer
{
public:
	Producer(const unsigned int buffersize, boost::function<void (Assembly<Product> *target)> producer);

	virtual ~Producer();

	const Product get();

	unsigned int available_count() const;

private:
	Assembly<Product> _products;

	boost::thread _producer;
};

template<class Product>
inline Producer<Product>::Producer(
		const unsigned int buffersize, boost::function<void (Assembly<Product> *target)> producer) :
	_products(buffersize), _producer(boost::bind(producer,&_products))
{
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
