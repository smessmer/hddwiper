#pragma once

#ifndef __ASSEMBLY_HPP__
#define __ASSEMBLY_HPP__

#include "util/thread/ThreadsafeQueue.hpp"
#include "util/thread/Semaphore.hpp"

template<class Product>
class Assembly
{
public:
	Assembly(const unsigned int size);

	void push(const Product &data);
	const Product pop();

	unsigned int size() const;
private:
	ThreadsafeQueue<Product> _queue;
	Semaphore _empty;
	Semaphore _full;
};

template<class Product>
inline Assembly<Product>::Assembly(const unsigned int size)
	:_queue(),_empty(size),_full(0)
{
}

template<class Product>
inline void Assembly<Product>::push(const Product &data)
{
	//Wait until one slot is empty
	_empty.wait();
	//Insert data into that slot
	_queue.push(data);
	//Notify that this slot is full
	_full.release();
}

template<class Product>
inline const Product Assembly<Product>::pop()
{
	//Wait until one slot is filled
	_full.wait();
	//Request this data
	const Product result=_queue.pop();
	//Notify that this slot is empty
	_empty.release();

	return result;
}

template<class Product>
inline unsigned int Assembly<Product>::size() const
{
	return _queue.size();
}

#endif /* __ASSEMBLY_HPP__ */
