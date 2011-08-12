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

inline RC4StreamProducer::~RC4StreamProducer()
{
	//The producer thread started by Producer<T> has a callback function
	//calling a function in RC4StreamProducer.
	//Producer<T>::~Producer waits for this thread to stop, but
	//while waiting the RC4StreamProducer object is already
	//destroyed, so the callback function will call a function of
	//a destroyed object => not so good.
	//So be sure that the producer thread is stopped, before
	//RC4StreamProducer is destroyed.
	stop();
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
