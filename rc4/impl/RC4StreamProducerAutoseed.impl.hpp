inline RC4StreamProducerAutoseed::RC4StreamProducerAutoseed(
		const unsigned int buffersize, const unsigned int blocksize, const unsigned int blocks_per_seed) :
	RC4StreamProducer(buffersize,blocksize),_blocks_per_seed(blocks_per_seed),_count_until_reseed(0),_entropyproducer(SEEDCOUNT,SEEDSIZE)
{
}

inline unsigned int RC4StreamProducerAutoseed::available_seed() const
{
	return _entropyproducer.available_count();
}

inline unsigned int RC4StreamProducerAutoseed::seeding_status() const
{
	return _entropyproducer.seeding_status();
}

inline void RC4StreamProducerAutoseed::BeforeProduce()
{
	if(_count_until_reseed==0)
	{
		reseed(_entropyproducer.get());
		_count_until_reseed=_blocks_per_seed;
	}
	--_count_until_reseed;
}

inline RC4StreamProducerAutoseed::~RC4StreamProducerAutoseed()
{
	//The producer thread started by Producer<T> has a callback function
	//calling a function in RC4StreamProducer which calls a virtual
	//function of RC4StreamProducerAutoseed.
	//Producer<T>::~Producer waits for this thread to stop, but
	//while waiting the RC4StreamProducerAutoseed object is already
	//destroyed, so the callback function will call a function of
	//a destroyed object => not so good.
	//So be sure that the producer thread is stopped, before
	//RC4StreamProducerAutoseed is destroyed.
	stop();
}
