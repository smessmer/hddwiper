inline RC4Streamgenerator::RC4Streamgenerator(const unsigned int blocksize, const Data &seed) :
	_zeroes(blocksize), _key()
{
	memset(_zeroes.get(), 0, _zeroes.size());
	reseed(seed);
}

inline RC4Streamgenerator::RC4Streamgenerator(const unsigned int blocksize) :
	_zeroes(blocksize), _key()
{
	memset(_zeroes.get(), 0, _zeroes.size());
}

inline void RC4Streamgenerator::reseed(const Data &seeddata)
{
	RC4_set_key(&_key, seeddata.size(), seeddata.get());
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

	RC4(&_key, _zeroes.size(), _zeroes.get(), data.get());
}
