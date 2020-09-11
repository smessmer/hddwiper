#pragma once

#ifndef RANDOMSTREAMPRODUCER_HPP_
#define RANDOMSTREAMPRODUCER_HPP_

#include "producer/Producer.hpp"
#include "util/data/Data.hpp"

#include <boost/thread.hpp>
#include <randomstream/RandomStreamGenerator.hpp>

/**
 * This class is able to produce a random stream in another thread
 * (used the producer/consumer pattern) and make it available
 * to the current thread.
 *
 * @author Sebastian Meßmer
 */
class RandomStreamProducer: public Producer<Data>
{
public:
	/**
	 * Create a new random stream producer and run it.
	 * It will immediately start another thread and
	 * generate random data.
	 *
	 * @param buffersize
	 * 		The number of blocks to store in buffer.
	 *		When the buffer is full and no one fetches the data,
	 *		the generator thread will pause, until a block of random
	 *		data is fetched.
	 * @param blocksize
	 * 		The number of bytes one block of random data contains.
	 * @param seed The seed (also called key or init vector) for the random stream algorithm
	 *
	 */
	RandomStreamProducer(Assembly<Data>* random_block_output_assembly,
			const unsigned int blocksize, const Data &seed);

	/**
	 * Destructor
	 */
	virtual ~RandomStreamProducer();

protected:

	/**
	 * Create a new random stream producer and run it.
	 * When you use this constructor, you have to
	 * overwrite BeforeProduce() and seed the RandomStreamProducer
	 * by a call to reseed() at the first call of BeforeProduce().
	 *
	 * @param random_block_output_assembly
	 * 		The assembly to push the produced random blocks to
	 * @param blocksize
	 * 		The number of bytes one block of random data contains.
	 *
	 */
	RandomStreamProducer(Assembly<Data>* random_block_output_assembly, unsigned int blocksize);

	/**
	 * Restarts the random stream generator with a new seed.
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
	Data _generate();

	RandomStreamGenerator _generator;
};

#include <randomstream/impl/RandomStreamProducer.impl.hpp>

#endif /* RANDOMSTREAMPRODUCER_HPP_ */
