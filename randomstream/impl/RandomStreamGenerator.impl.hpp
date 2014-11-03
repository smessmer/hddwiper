#include <crypto++/sosemanuk.h>

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

inline const Data RandomStreamGenerator::getRandomBytes()
{
	Data result(_zeroes.size());
	getRandomBytes(result);
	return result;
}

inline const void RandomStreamGenerator::getRandomBytes(Data &data)
{
	if(data.size()!=_zeroes.size())
		throw std::logic_error("Too small (or too large) memory region given for writing one block of random data");

	_cipher->ProcessData(data.get(), _zeroes.get(), _zeroes.size());
}

inline unsigned int RandomStreamGenerator::SeedSize()
{
	return CryptoPP::Sosemanuk::MAX_KEYLENGTH + CryptoPP::Sosemanuk::IV_LENGTH;
}
