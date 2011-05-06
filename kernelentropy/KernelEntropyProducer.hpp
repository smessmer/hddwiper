#pragma once

#ifndef KERNELENTROPYPRODUCER_HPP_
#define KERNELENTROPYPRODUCER_HPP_

#include "util/DataAssembly.hpp"
#include "KernelEntropy.hpp"

#include <boost/thread.hpp>

class KernelEntropyProducer
{
public:
	KernelEntropyProducer(const unsigned int buffersize,
			const unsigned int blocksize);

	~KernelEntropyProducer();

	const Data get();

	unsigned int available_count() const;

private:
	DataAssembly _entropy;

	boost::thread _producer;

	class ProducerThread
	{
	public:
		ProducerThread(DataAssembly &target, const unsigned int blocksize);
		void operator()();
	private:
		const unsigned int _blocksize;
		DataAssembly &_target;
	};
};

inline KernelEntropyProducer::KernelEntropyProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	_entropy(buffersize), _producer(ProducerThread(_entropy, blocksize))
{
}

inline KernelEntropyProducer::~KernelEntropyProducer()
{
	_producer.interrupt();
	_producer.join();
}

inline const Data KernelEntropyProducer::get()
{
	return _entropy.pop();
}

inline unsigned int KernelEntropyProducer::available_count() const
{
	return _entropy.size();
}

inline KernelEntropyProducer::ProducerThread::ProducerThread(DataAssembly &target,
		const unsigned int blocksize) :
	_blocksize(blocksize), _target(target)
{
}

inline void KernelEntropyProducer::ProducerThread::operator()()
{
	try
	{
		while (true)
		{
			_target.push(KernelEntropy::getEntropy(_blocksize));
			boost::this_thread::interruption_point();
		}
	}
	catch(boost::thread_interrupted &interruptedexception)
	{
	}
}

#endif /* KERNELENTROPYPRODUCER_HPP_ */
