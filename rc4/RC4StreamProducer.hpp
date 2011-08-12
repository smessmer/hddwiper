#pragma once

#ifndef RC4STREAMPRODUCER_HPP_
#define RC4STREAMPRODUCER_HPP_

#include "producer/Producer.hpp"
#include "RC4Streamgenerator.hpp"
#include "util/data/Data.hpp"

#include <boost/thread.hpp>

/**
 * This class is able to produce a RC4 stream in another thread
 * (used the producer/consumer pattern) and make it available
 * to the current thread.
 *
 * @author Sebastian Meßmer
 */
class RC4StreamProducer: public Producer<Data>
{
public:
	/**
	 * Create a new RC4 stream producer and run it.
	 * It will immediately start another thread and
	 * generate RC4 random data.
	 *
	 * @param buffersize
	 * 		The number of blocks to store in buffer.
	 *		When the buffer is full and no one fetches the data,
	 *		the generator thread will pause, until a block of random
	 *		data is fetched.
	 * @param blocksize
	 * 		The number of bytes one block of random data contains.
	 * @param seed The seed (also called key or init vector) for the RC4 algorithm
	 *
	 */
	RC4StreamProducer(const unsigned int buffersize,
			const unsigned int blocksize, const Data &seed);

	/**
	 * Destructor
	 */
	virtual ~RC4StreamProducer();

protected:

	/**
	 * Create a new RC4 stream producer and run it.
	 * When you use this constructor, you have to
	 * overwrite BeforeProduce() and seed the RC4StreamProducer
	 * by a call to reseed() at the first call of BeforeProduce().
	 *
	 * @param buffersize
	 * 		The number of blocks to store in buffer.
	 *		When the buffer is full and no one fetches the data,
	 *		the generator thread will pause, until a block of random
	 *		data is fetched.
	 * @param blocksize
	 * 		The number of bytes one block of random data contains.
	 *
	 */
	RC4StreamProducer(const unsigned int buffersize, unsigned int blocksize);

	/**
	 * Restarts the RC4 stream generator with a new seed.
	 * This function should only be called out of the producing thread,
	 * this means out of OnProduced()
	 *
	 * @param seed The new seed to use
	 */
	void reseed(const Data &seed);

	/**
	 * This function can be overwritten and is called every time,
	 * before a new block of random data was produced. This function
	 * is called in the producing thread!
	 * It can be used to reseed the generator from time to time.
	 */
	virtual void BeforeProduce();

private:
	const Data _generate();

	RC4Streamgenerator _generator;
};

#include "impl/RC4StreamProducer.impl.hpp"

#endif /* RC4STREAMPRODUCER_HPP_ */
