inline RandomStreamProducer::RandomStreamProducer(
		Assembly<Data>* random_block_output_assembly, const unsigned int blocksize, const Data &seed)
	: Producer<Data>(random_block_output_assembly)
	, _generator(blocksize,seed)
{
	Producer<Data>::run(std::bind(&RandomStreamProducer::_generate,this));
}

inline RandomStreamProducer::RandomStreamProducer(
		Assembly<Data>* random_block_output_assembly, const unsigned int blocksize) :
	Producer<Data>(random_block_output_assembly),_generator(blocksize)
{
	Producer<Data>::run(std::bind(&RandomStreamProducer::_generate,this));
}

inline RandomStreamProducer::~RandomStreamProducer()
{
	//The producer thread started by Producer<T> has a callback function
	//calling a function in RandomStreamProducer.
	//Producer<T>::~Producer waits for this thread to stop, but
	//while waiting the RandomStreamProducer object is already
	//destroyed, so the callback function will call a function of
	//a destroyed object => not so good.
	//So be sure that the producer thread is stopped, before
	//RandomStreamProducer is destroyed.
	stop();
}

inline const Data RandomStreamProducer::_generate()
{
	BeforeProduce();
	return _generator.getRandomBytes();
}

inline void RandomStreamProducer::reseed(const Data &seed)
{
	_generator.reseed(seed);
}

inline void RandomStreamProducer::BeforeProduce()
{
	//Intentionally blank
}
