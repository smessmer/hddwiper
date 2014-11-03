#pragma once

#ifndef __RC4STREAMGENERATOR_HPP__
#define __RC4STREAMGENERATOR_HPP__

#include <functional>
#include <openssl/rc4.h>

#include "util/data/Data.hpp"

/**
 * This class is able to generate a RC4 stream.
 *
 * @author Sebastian Meßmer
 */
class RC4Streamgenerator
{
public:
	/**
	 * Create a new RC4 stream generator and seed it
	 *
	 * @param blocksize The amount of random data that is fetched at once from the stream
	 * @param seed
	 * 		The seeding value (key) for the RC4 algorithm.
     *  	The length should be between 1 and 256 bytes.
	 */
	RC4Streamgenerator(const unsigned int blocksize, const Data &seed);

	/**
	 * Create a new RC4 stream generator but don't seed it
	 *
	 * @param blocksize The amount of random data that is fetched at once from the stream
	 */
	RC4Streamgenerator(const unsigned int blocksize);

	/**
	 * Return the next block of random data
	 * @return The next block of random data
	 */
	const Data getRandomBytes();

	/**
	 * Write the next block of random data into the given memory
	 * @param data The memory where to write to
	 */
	const void getRandomBytes(Data &data);

	/**
	 * Restart the stream generator with the given seed (key)
	 * @param seeddata
	 * 		The seeding value (key) for the RC4 algorithm.
     *  	The length should be between 1 and 256 bytes.
	 */
	void reseed(const Data &seeddata);
private:
	Data _zeroes;
	RC4_KEY _key;
};

#include "impl/RC4Streamgenerator.impl.hpp"

#endif
