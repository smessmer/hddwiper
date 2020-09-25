#pragma once

#ifndef __RANDOMSTREAMGENERATOR_HPP__
#define __RANDOMSTREAMGENERATOR_HPP__

#include <functional>
#include <crypto++/cryptlib.h>
#include <memory>

#include "util/data/Data.hpp"

/**
 * This class is able to generate a random stream.
 *
 * @author Sebastian Meßmer
 */
class RandomStreamGenerator
{
public:
	/**
	 * Create a new random stream generator and seed it
	 *
	 * @param blocksize The amount of random data that is fetched at once from the stream
	 * @param seed
	 * 		The seeding value (key) for the random stream algorithm.
     *  	The length should fit the return value of SeedSize().
	 */
	RandomStreamGenerator(const unsigned int blocksize, const Data &seed, bool disable_rdrand);

	/**
	 * Create a new random stream generator but don't seed it
	 *
	 * @param blocksize The amount of random data that is fetched at once from the stream
	 */
	RandomStreamGenerator(const unsigned int blocksize, bool disable_rdrand);

	/**
	 * Return the next block of random data
	 * @return The next block of random data
	 */
	Data getRandomBytes();

	/**
	 * Write the next block of random data into the given memory
	 * @param data The memory where to write to
	 */
	void getRandomBytes(Data &data);

	/**
	 * Restart the stream generator with the given seed (key)
	 * @param seeddata
	 * 		The seeding value (key) for the random stream algorithm.
     *  	The length should fit the return value of SeedSize().
	 */
	void reseed(const Data &seeddata);

	/**
	 * Return the seed size we need(in bytes)
	 */
	static unsigned int SeedSize();
private:
	Data _zeroes;
	std::unique_ptr<CryptoPP::SymmetricCipher> _cipher;
        bool _disable_rdrand;
};

#include <randomstream/impl/RandomStreamGenerator.impl.hpp>

#endif
