inline RC4StreamProducer::RC4StreamProducer(
		const unsigned int buffersize, const unsigned int blocksize, const Data &seed) :
	Producer<Data>(buffersize),_generator(blocksize,seed)
{
	Producer<Data>::run(boost::bind(&RC4StreamProducer::_generate,this));
}

inline RC4StreamProducer::RC4StreamProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	Producer<Data>(buffersize),_generator(blocksize)
{
	Producer<Data>::run(boost::bind(&RC4StreamProducer::_generate,this));
}

inline const Data RC4StreamProducer::_generate()
{
	BeforeProduce();
	return _generator.getRandomBytes();
}

inline void RC4StreamProducer::reseed(const Data &seed)
{
	_generator.reseed(seed);
}

inline void RC4StreamProducer::BeforeProduce()
{
	//Intentionally blank
}
