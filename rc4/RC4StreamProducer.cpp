#include "RC4StreamProducer.hpp"

void RC4StreamProducer::ProducerThread::operator()(
		Assembly<Data> *target)
{
	try
	{
		_entropyproducer=std::tr1::shared_ptr<KernelEntropyProducer>(new KernelEntropyProducer(SEEDCOUNT,SEEDSIZE));

		int count_until_reseed=0;
		while (true)
		{
			//TODO reseed nicht nur abhängig von blockcount, sondern auch von blocksize (=> bytecount)
			if(count_until_reseed==0)
			{
//std::cout << "reseed start"<<std::endl;
				reseed();
//std::cout << "reseed end"<<std::endl;
				count_until_reseed=100;
			}
			--count_until_reseed;
//std::cout << "random start"<<std::endl;
			target->push(_generator.getRandomBytes());
//std::cout << "random end"<<std::endl;
			boost::this_thread::interruption_point();
		}
	} catch (boost::thread_interrupted &interruptedexception)
	{
	}
}
