#include "RC4StreamProducer.hpp"

Data RC4StreamProducer::ProducerThread::operator()()
{
	//TODO reseed nicht nur abhängig von blockcount, sondern auch von blocksize (=> bytecount)
	if(_count_until_reseed==0)
	{
std::cout << "reseed start"<<std::endl;
		reseed();
std::cout << "reseed end"<<std::endl;
		_count_until_reseed=100;
	}
	--_count_until_reseed;
//std::cout << "random start"<<std::endl;
	return _generator.getRandomBytes();
//std::cout << "random end"<<std::endl;
}
