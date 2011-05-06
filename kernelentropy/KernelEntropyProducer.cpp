#include "KernelEntropyProducer.hpp"

void KernelEntropyProducer::ProducerThread::operator()(
		Assembly<Data> *target)
{
	try
	{
		while (true)
		{
			_seeding_status=0;
			target->push(KernelEntropy::getEntropy(_blocksize,boost::bind(&KernelEntropyProducer::ProducerThread::inc_seedingstatus,this)));
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}
