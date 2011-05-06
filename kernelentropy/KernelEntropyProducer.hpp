#pragma once

#ifndef KERNELENTROPYPRODUCER_HPP_
#define KERNELENTROPYPRODUCER_HPP_

#include "producer/Producer.hpp"
#include "KernelEntropy.hpp"
#include "util/data/Data.hpp"

#include <boost/thread.hpp>

class KernelEntropyProducer: public Producer<Data>
{
public:
	KernelEntropyProducer(const unsigned int buffersize,
			const unsigned int blocksize);

private:

	class ProducerThread
	{
	public:
		ProducerThread(const unsigned int blocksize);
		void operator()(Assembly<Data> *target);
	private:
		const unsigned int _blocksize;
	};
};

inline KernelEntropyProducer::KernelEntropyProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	Producer<Data>(buffersize,ProducerThread(blocksize))
{
}

inline KernelEntropyProducer::ProducerThread::ProducerThread(
		const unsigned int blocksize) :
	_blocksize(blocksize)
{
}

inline void KernelEntropyProducer::ProducerThread::operator()(
		Assembly<Data> *target)
{
	try
	{
		while (true)
		{
			target->push(KernelEntropy::getEntropy(_blocksize));
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}

#endif /* KERNELENTROPYPRODUCER_HPP_ */
