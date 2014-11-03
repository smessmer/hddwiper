#pragma once

#ifndef RC4STREAMPRODUCERAUTOSEED_HPP_
#define RC4STREAMPRODUCERAUTOSEED_HPP_

#include "RC4StreamProducer.hpp"
#include "kernelentropy/KernelEntropyProducer.hpp"

/**
 * This class is able to produce a RC4 stream in another thread
 * (used the producer/consumer pattern) and make it available
 * to the current thread.
 * This class is automatically seeding itself from kernel entropy.
 * It also takes a new seed from time to time.
 *
 * @author Sebastian Meßmer
 */
class RC4StreamProducerAutoseed: public RC4StreamProducer
{
public:
	/**
	 * Create a new RC4 stream producer and run it.
	 * It will immediately seed itself, start another thread and
	 * generate RC4 random data.
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
	RC4StreamProducerAutoseed(const unsigned int buffersize,
			const unsigned int blocksize, const unsigned int blocks_per_seed);

	/**
	 * Destructor
	 */
	virtual ~RC4StreamProducerAutoseed();

	/**
	 * Return the current available count of seed blocks,
	 * which can be used for future reseeding.
	 *
	 * @return The current available count of seed blocks
	 */
	unsigned int available_seed() const;

	/**
	 * Return the current number of bytes that are received for creating the current seed block.
	 *
	 * @return The current number of bytes that are received for creating the current seed block.
	 */
	unsigned int seeding_status() const;

private:

	static const unsigned int SEEDCOUNT=200;

	int _blocks_per_seed;
	int _count_until_reseed;

	KernelEntropyProducer _entropyproducer;

	virtual void BeforeProduce();
};

#include "impl/RC4StreamProducerAutoseed.impl.hpp"

#endif /* RC4STREAMPRODUCERWITHRESEEDING_HPP_ */
