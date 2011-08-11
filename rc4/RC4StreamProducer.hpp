#pragma once

#ifndef RC4STREAMPRODUCER_HPP_
#define RC4STREAMPRODUCER_HPP_

#include "producer/Producer.hpp"
#include "RC4Streamgenerator.hpp"
#include "kernelentropy/KernelEntropyProducer.hpp"
#include "util/data/Data.hpp"

#include <boost/thread.hpp>

class RC4StreamProducer: public Producer<Data>
{
public:
	RC4StreamProducer(const unsigned int buffersize,
			const unsigned int blocksize);

	unsigned int available_seed() const;

	unsigned int seeding_status() const;

private:

	class ProducerThread
	{
	public:
		ProducerThread(const unsigned int blocksize);
		Data operator()();

		unsigned int available_seed() const;

		unsigned int seeding_status() const;
	private:
		void reseed();

		static const unsigned int SEEDSIZE=256;
		static const unsigned int SEEDCOUNT=200;

		KernelEntropyProducer _entropyproducer;

		RC4Streamgenerator _generator;

		int _count_until_reseed;
	};

	ProducerThread _producerthread;
};

inline RC4StreamProducer::RC4StreamProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	Producer<Data>(buffersize),_producerthread(blocksize)
{
	run(boost::ref(_producerthread));
}

inline RC4StreamProducer::ProducerThread::ProducerThread(
		const unsigned int blocksize) :
	_entropyproducer(SEEDCOUNT,SEEDSIZE),_generator(blocksize),_count_until_reseed(0)
{
}

inline void RC4StreamProducer::ProducerThread::reseed()
{
	_generator.reseed(_entropyproducer.get());
}

inline unsigned int RC4StreamProducer::available_seed() const
{
	return _producerthread.available_seed();
}

inline unsigned int RC4StreamProducer::seeding_status() const
{
	return _producerthread.seeding_status();
}

inline unsigned int RC4StreamProducer::ProducerThread::available_seed() const
{
	return _entropyproducer.available_count();
}

inline unsigned int RC4StreamProducer::ProducerThread::seeding_status() const
{
	return _entropyproducer.seeding_status();
}

#endif /* RC4STREAMPRODUCER_HPP_ */
