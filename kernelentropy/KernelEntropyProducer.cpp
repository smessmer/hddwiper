#include "KernelEntropyProducer.hpp"

void KernelEntropyProducer::ProducerThread::operator()(
		Assembly<Data> *target)
{
	try
	{
		while (true)
		{
			target->push(KernelEntropy::getEntropy(_blocksize));
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}
