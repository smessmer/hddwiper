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

private:

	class ProducerThread
	{
	public:
		ProducerThread(const unsigned int blocksize);
		void operator()(Assembly<Data> *target);
	private:
		void reseed();

		static const unsigned int SEEDSIZE=256;
		static const unsigned int SEEDCOUNT=2;

		std::tr1::shared_ptr<KernelEntropyProducer> _entropyproducer;

		RC4Streamgenerator _generator;
	};
};

inline RC4StreamProducer::RC4StreamProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	Producer<Data>(buffersize,ProducerThread(blocksize))
{
}

inline RC4StreamProducer::ProducerThread::ProducerThread(
		const unsigned int blocksize) :
	_entropyproducer(new KernelEntropyProducer(SEEDCOUNT,SEEDSIZE)),_generator(blocksize,_entropyproducer->get())
{
}

inline void RC4StreamProducer::ProducerThread::reseed()
{
	_generator.reseed(_entropyproducer->get());
}

#endif /* RC4STREAMPRODUCER_HPP_ */
