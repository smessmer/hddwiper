#pragma once

#ifndef RANDOMSTREAMPRODUCERAUTOSEED_HPP_
#define RANDOMSTREAMPRODUCERAUTOSEED_HPP_

#include <randomstream/RandomStreamProducer.hpp>
#include "kernelentropy/KernelEntropyProducer.hpp"

/**
 * This class is able to produce a random stream in another thread
 * (used the producer/consumer pattern) and make it available
 * to the current thread.
 * This class is automatically seeding itself from kernel entropy.
 * It also takes a new seed from time to time.
 *
 * @author Sebastian Meßmer
 */
class RandomStreamProducerAutoseed: public RandomStreamProducer
{
public:
	/**
	 * Create a new random stream producer and run it.
	 * It will immediately seed itself, start another thread and
	 * generate random random data.
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
	RandomStreamProducerAutoseed(Assembly<Data>* random_block_output_assembly,
			const unsigned int blocksize, const unsigned int blocks_per_seed,
			Assembly<Data>* seed_block_input_assembly);

	/**
	 * Destructor
	 */
	virtual ~RandomStreamProducerAutoseed();

	// /**
	//  * Return the current available count of seed blocks,
	//  * which can be used for future reseeding.
	//  *
	//  * @return The current available count of seed blocks
	//  */
	// unsigned int available_seed() const;
	//
	// /**
	//  * Return the progress percentage for creating the current seed block.
	//  *
	//  * @return A number x with 0.0<=x<=1.0
	//  */
	// double seeding_status() const;

private:

	int _blocks_per_seed;
	int _count_until_reseed;

  Assembly<Data>* _seed_block_input_assembly;

	virtual void BeforeProduce();
};

#include <randomstream/impl/RandomStreamProducerAutoseed.impl.hpp>

#endif /* RANDOMSTREAMPRODUCERAUTOSEED_HPP_ */
