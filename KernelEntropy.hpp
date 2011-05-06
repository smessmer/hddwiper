#pragma once

#ifndef __KERNELENTROPY_HPP__
#define __KERNELENTROPY_HPP__

#include <cstdio>
//TODO Exceptions instead of cerr
#include <iostream>

#include "DummyCallback.hpp"

class KernelEntropy
{
public:
	static const Data getEntropy(const unsigned int size, boost::function<void(
			unsigned int)> callback = DummyCallback());
};

inline const Data KernelEntropy::getEntropy(const unsigned int size,
		boost::function<void(unsigned int)> callback)
{
	Data result(size);

	FILE *file = fopen("/dev/random", "rb");
	if (file == NULL)
		std::cerr << "Error opening /dev/random" << std::endl;
	for (unsigned int i = 0; i < size; ++i)
	{
		result.get()[i] = fgetc(file);
		if (ferror(file))
			std::cerr << "Error seeding from /dev/random" << std::endl;
		callback(i);
	}
	fclose(file);

	return result;
}

#endif
