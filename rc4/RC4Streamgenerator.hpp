#pragma once

#ifndef __RC4STREAMGENERATOR_HPP__
#define __RC4STREAMGENERATOR_HPP__

#include <openssl/rc4.h>
#include <boost/function.hpp>

#include "util/data/Data.hpp"
#include "util/DummyCallback.hpp"
#include "kernelentropy/KernelEntropy.hpp"

class RC4Streamgenerator
{
public:
	RC4Streamgenerator(const unsigned int blocksize, const Data &seed);

	const Data getRandomBytes();
	const void getRandomBytes(Data &data);

	void reseed(const Data &seeddata);
private:
	const unsigned int _blocksize;
	Data _zeroes;
	RC4_KEY _key;
};

inline RC4Streamgenerator::RC4Streamgenerator(const unsigned int blocksize, const Data &seed) :
	_blocksize(blocksize),_zeroes(_blocksize), _key()
{
	memset(_zeroes.get(), 0, _blocksize);
	reseed(seed);
}

inline void RC4Streamgenerator::reseed(const Data &seeddata)
{
	RC4_set_key(&_key, seeddata.size(), seeddata.get());
}

inline const Data RC4Streamgenerator::getRandomBytes()
{
	Data result(_blocksize);
	getRandomBytes(result);
	return result;
}

inline const void RC4Streamgenerator::getRandomBytes(Data &data)
{
	RC4(&_key, _blocksize, _zeroes.get(), data.get());
}

#endif
