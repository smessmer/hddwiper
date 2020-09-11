#include "../KernelEntropy.hpp"
#include <cryptopp/cpu.h>
#include <cryptopp/rdrand.h>

#include <cstdio>
#include <stdexcept>

Data KernelEntropy::getEntropy(const unsigned int size,
		std::function<void(unsigned int)> callback)
{
	Data result(size);
	if (CryptoPP::HasRDSEED()) {
		CryptoPP::RDSEED().GenerateBlock(result.get(), result.size());
	} else {
		FILE *file = fopen("/dev/random", "rb");
		if (file == NULL)
			throw std::runtime_error("Error opening /dev/random");
		for (unsigned int i = 0; i < size; ++i)
		{
			result.get()[i] = fgetc(file);
			if (ferror(file))
				throw std::runtime_error("Error seeding from /dev/random");
			callback(i);
		}
		fclose(file);
	}
	return result;
}
