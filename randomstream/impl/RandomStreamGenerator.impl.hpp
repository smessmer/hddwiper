#include <crypto++/sosemanuk.h>
#include <cryptopp/rdrand.h>
#include <cryptopp/cpu.h>
#include <iostream>
inline RandomStreamGenerator::RandomStreamGenerator(const unsigned int blocksize, const Data &seed) :
	_zeroes(blocksize), _cipher(nullptr)
{
	memset(_zeroes.get(), 0, _zeroes.size());
	reseed(seed);
}

inline RandomStreamGenerator::RandomStreamGenerator(const unsigned int blocksize) :
	_zeroes(blocksize), _cipher()
{
	memset(_zeroes.get(), 0, _zeroes.size());
}

inline void RandomStreamGenerator::reseed(const Data &seeddata)
{
	//Split the seed data into key (first part) and IV (second part)
	constexpr unsigned int KEYLENGTH = CryptoPP::Sosemanuk::MAX_KEYLENGTH;
	assert(seeddata.size() == SeedSize());
	const unsigned char *key = seeddata.get();
	const unsigned char *iv = seeddata.get() + KEYLENGTH;

  _cipher = std::make_unique<CryptoPP::Sosemanuk::Encryption>(key, KEYLENGTH, iv);
}

inline Data RandomStreamGenerator::getRandomBytes()
{
	Data result(_zeroes.size());
	getRandomBytes(result);
	return result;
}

inline void RandomStreamGenerator::getRandomBytes(Data &data)
{
	if(data.size()!=_zeroes.size())
		throw std::logic_error("Too small (or too large) memory region given for writing one block of random data");

	if (nullptr == _cipher)
		throw std::logic_error("Not seeded yet");

    // Get Sosemanuk data
	_cipher->ProcessData(data.get(), _zeroes.get(), _zeroes.size());

	// XOR with RDRAND data
	if (CryptoPP::HasRDRAND()) {
		Data rdrand_data(_zeroes.size());
		CryptoPP::RDRAND().GenerateBlock(rdrand_data.get(), rdrand_data.size());
		for (size_t i = 0; i < _zeroes.size(); ++i) {
			_zeroes.get()[i] ^= rdrand_data.get()[i];
		}
	}
}

inline unsigned int RandomStreamGenerator::SeedSize()
{
	return CryptoPP::Sosemanuk::MAX_KEYLENGTH + CryptoPP::Sosemanuk::IV_LENGTH;
}
