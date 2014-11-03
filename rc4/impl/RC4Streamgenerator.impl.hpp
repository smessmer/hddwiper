#include <crypto++/sosemanuk.h>

inline RC4Streamgenerator::RC4Streamgenerator(const unsigned int blocksize, const Data &seed) :
	_zeroes(blocksize), _cipher(nullptr)
{
	memset(_zeroes.get(), 0, _zeroes.size());
	reseed(seed);
}

inline RC4Streamgenerator::RC4Streamgenerator(const unsigned int blocksize) :
	_zeroes(blocksize), _cipher()
{
	memset(_zeroes.get(), 0, _zeroes.size());
}

inline void RC4Streamgenerator::reseed(const Data &seeddata)
{
	//Split the seed data into key (first part) and IV (second part)
	assert(seeddata.size() == SeedSize());
	const unsigned char *key = seeddata.get();
	const unsigned char *iv = seeddata.get() + CryptoPP::Sosemanuk::MAX_KEYLENGTH;

    _cipher = std::make_unique<CryptoPP::Sosemanuk::Encryption>(key, CryptoPP::Sosemanuk::MAX_KEYLENGTH, iv);
}

inline const Data RC4Streamgenerator::getRandomBytes()
{
	Data result(_zeroes.size());
	getRandomBytes(result);
	return result;
}

inline const void RC4Streamgenerator::getRandomBytes(Data &data)
{
	if(data.size()!=_zeroes.size())
		throw std::logic_error("Too small (or too large) memory region given for writing one block of random data");

	_cipher->ProcessData(data.get(), _zeroes.get(), _zeroes.size());
}

inline unsigned int RC4Streamgenerator::SeedSize()
{
	return CryptoPP::Sosemanuk::MAX_KEYLENGTH + CryptoPP::Sosemanuk::IV_LENGTH;
}
