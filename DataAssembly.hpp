#pragma once

#ifndef __DATAASSEMBLY_HPP__
#define __DATAASSEMBLY_HPP__

#include "ThreadsafeQueue.hpp"
#include "Data.hpp"
#include "Semaphore.hpp"

class DataAssembly
{
public:
	DataAssembly(const unsigned int size);

	void push(const Data &data);
	const Data pop();
private:
	ThreadsafeQueue<Data> _queue;
	Semaphore _empty;
	Semaphore _full;
};

inline DataAssembly::DataAssembly(const unsigned int size)
	:_queue(),_empty(size),_full(0)
{
}

inline void DataAssembly::push(const Data &data)
{
	//Wait until one slot is empty
	_empty.wait();
	//Insert data into that slot
	_queue.push(data);
	//Notify that this slot is full
	_full.release();
}

inline const Data DataAssembly::pop()
{
	//Wait until one slot is filled
	_full.wait();
	//Request this data
	const Data result=_queue.pop();
	//Notify that this slot is empty
	_empty.release();

	return result;
}

#endif /* __DATAASSEMBLY_HPP__ */
