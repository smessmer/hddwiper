inline KernelEntropyProducer::KernelEntropyProducer(
		Assembly<Data>* seed_block_output_assembly, const unsigned int blocksize) :
	Producer<Data>(seed_block_output_assembly),_blocksize(blocksize),_seeding_status(0)
{
	run(std::bind(&KernelEntropyProducer::_generate,this));
}

inline KernelEntropyProducer::~KernelEntropyProducer()
{
	stop();
}

inline void KernelEntropyProducer::_set_seedingstatus(unsigned int seedingstatus)
{
	_seeding_status=seedingstatus;
}

inline Data KernelEntropyProducer::_generate()
{
	_seeding_status=0;
	return KernelEntropy::getEntropy(_blocksize,std::bind(&KernelEntropyProducer::_set_seedingstatus,this,std::placeholders::_1));
}

inline unsigned int KernelEntropyProducer::seeding_status() const
{
	return _seeding_status;
}
