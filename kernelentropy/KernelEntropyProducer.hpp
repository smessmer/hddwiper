#pragma once

#ifndef KERNELENTROPYPRODUCER_HPP_
#define KERNELENTROPYPRODUCER_HPP_

#include "producer/Producer.hpp"
#include "KernelEntropy.hpp"
#include "util/data/Data.hpp"
#include "util/thread/Threadsafe.hpp"

#include <boost/thread.hpp>

/**
 * This class is able to produce kernel entropy in another thread
 * and make it available to the current thread. So the current thread
 * does not have to pause while waiting for the kernel entropy.
 *
 * @author Sebastian Meßmer
 */
class KernelEntropyProducer: public Producer<Data>
{
public:
	/**
	 * Create a new kernel entropy producer.
	 * This constructor immediately starts a new
	 * thread which is generating entropy.
	 *
	 * @param buffersize
	 * 		The number of blocks to store in buffer.
	 * 		If the buffer is full and nobody fetches its content,
	 * 		the producer thread pauses until a block of data is fetched.
	 * @param blocksize
	 * 		The number of bytes one block of entropy data contains.
	 */
	KernelEntropyProducer(const unsigned int buffersize,
			const unsigned int blocksize);

	/**
	 * Return the number of bytes that are currently received for generating
	 * the current block. This is a value between 0 and the blocksize given
	 * in the constructor. It can be used for a progress bar to show the
	 * progress in generating the current block of kernel entropy.
	 *
	 * @return The number of bytes that are currently received for generating the current block.
	 */
	unsigned int seeding_status() const;

private:

	//This function is called by the producer thread to set the current seeding status
	void _set_seedingstatus(unsigned int seedingstatus);
	//This function is called by the producer thread to get the next block of entropy
	const Data _generate();

	const unsigned int _blocksize;
	Threadsafe<unsigned int> _seeding_status;
};

#include "impl/KernelEntropyProducer.impl.hpp"

#endif /* KERNELENTROPYPRODUCER_HPP_ */
