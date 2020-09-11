inline RandomStreamProducerAutoseed::RandomStreamProducerAutoseed(
	    Assembly<Data>* random_block_output_assembly,
			const unsigned int blocksize, const unsigned int blocks_per_seed,
			Assembly<Data>* seed_block_input_assembly)
	: RandomStreamProducer(random_block_output_assembly,blocksize)
	, _blocks_per_seed(blocks_per_seed)
	, _count_until_reseed(0)
	, _seed_block_input_assembly(seed_block_input_assembly)
{
}

// inline unsigned int RandomStreamProducerAutoseed::available_seed() const
// {
// 	return _entropyproducer.available_count();
// }
//
// inline double RandomStreamProducerAutoseed::seeding_status() const
// {
// 	return static_cast<double>(_entropyproducer.seeding_status())/RandomStreamGenerator::SeedSize();
// }

inline void RandomStreamProducerAutoseed::BeforeProduce()
{
	if(_count_until_reseed==0)
	{
		reseed(_seed_block_input_assembly->pop());
		_count_until_reseed=_blocks_per_seed;
	}
	--_count_until_reseed;
}

inline RandomStreamProducerAutoseed::~RandomStreamProducerAutoseed()
{
	//The producer thread started by Producer<T> has a callback function
	//calling a function in RandomStreamProducer which calls a virtual
	//function of RandomStreamProducerAutoseed.
	//Producer<T>::~Producer waits for this thread to stop, but
	//while waiting the RandomStreamProducerAutoseed object is already
	//destroyed, so the callback function will call a function of
	//a destroyed object => not so good.
	//So be sure that the producer thread is stopped, before
	//RandomStreamProducerAutoseed is destroyed.
	stop();
}
