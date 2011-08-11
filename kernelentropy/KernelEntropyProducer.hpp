#pragma once

#ifndef KERNELENTROPYPRODUCER_HPP_
#define KERNELENTROPYPRODUCER_HPP_

#include "producer/Producer.hpp"
#include "KernelEntropy.hpp"
#include "util/data/Data.hpp"
#include "util/thread/Threadsafe.hpp"

#include <boost/thread.hpp>

class KernelEntropyProducer: public Producer<Data>
{
public:
	KernelEntropyProducer(const unsigned int buffersize,
			const unsigned int blocksize);

	unsigned int seeding_status() const;

private:

	class ProducerThread
	{
	public:
		ProducerThread(const unsigned int blocksize);
		Data operator()();

		unsigned int seeding_status() const;
	private:
		void inc_seedingstatus();
		const unsigned int _blocksize;
		Threadsafe<unsigned int> _seeding_status;
	};

	ProducerThread _producerthread;


};

inline KernelEntropyProducer::KernelEntropyProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	Producer<Data>(buffersize),  _producerthread(blocksize)
{
	run(boost::ref(_producerthread));
}

inline KernelEntropyProducer::ProducerThread::ProducerThread(
		const unsigned int blocksize) :
	_blocksize(blocksize),_seeding_status(0)
{
}

inline unsigned int KernelEntropyProducer::ProducerThread::seeding_status() const
{
	return _seeding_status;
}

inline void KernelEntropyProducer::ProducerThread::inc_seedingstatus()
{
	++_seeding_status;
}

inline Data KernelEntropyProducer::ProducerThread::operator()()
{
	_seeding_status=0;
	return (KernelEntropy::getEntropy(_blocksize,boost::bind(&KernelEntropyProducer::ProducerThread::inc_seedingstatus,this)));
	//return Data(256);
}

inline unsigned int KernelEntropyProducer::seeding_status() const
{
	return _producerthread.seeding_status();
}

#endif /* KERNELENTROPYPRODUCER_HPP_ */
