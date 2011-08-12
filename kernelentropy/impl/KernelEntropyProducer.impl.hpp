inline KernelEntropyProducer::KernelEntropyProducer(
		const unsigned int buffersize, const unsigned int blocksize) :
	Producer<Data>(buffersize),_blocksize(blocksize),_seeding_status(0)
{
	run(boost::bind(&KernelEntropyProducer::_generate,this));
}

inline void KernelEntropyProducer::_set_seedingstatus(unsigned int seedingstatus)
{
	_seeding_status=seedingstatus;
}

inline const Data KernelEntropyProducer::_generate()
{
	_seeding_status=0;
	return KernelEntropy::getEntropy(_blocksize,boost::bind(&KernelEntropyProducer::_set_seedingstatus,this,_1));
	//return Data(256);
}

inline unsigned int KernelEntropyProducer::seeding_status() const
{
	return _seeding_status;
}
