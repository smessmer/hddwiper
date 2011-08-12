inline RC4StreamProducerAutoseed::RC4StreamProducerAutoseed(
		const unsigned int buffersize, const unsigned int blocksize) :
	RC4StreamProducer(buffersize,blocksize),_count_until_reseed(0),_entropyproducer(SEEDCOUNT,SEEDSIZE)
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
		_count_until_reseed=100;
	}
	--_count_until_reseed;
}
