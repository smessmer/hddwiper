#include "RC4StreamProducer.hpp"

void RC4StreamProducer::ProducerThread::operator()(
		Assembly<Data> *target)
{
	try
	{
		_entropyproducer=std::tr1::shared_ptr<KernelEntropyProducer>(new KernelEntropyProducer(SEEDCOUNT,SEEDSIZE));
		reseed();

		int count_until_reseed=0;
		while (true)
		{
			//TODO reseed nicht nur abhängig von blockcount, sondern auch von blocksize (=> bytecount)
			if(count_until_reseed==0)
			{
				reseed();
				count_until_reseed=1000;
			}
			--count_until_reseed;

			target->push(_generator.getRandomBytes());
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}
