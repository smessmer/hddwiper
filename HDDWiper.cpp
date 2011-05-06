#include "HDDWiper.hpp"

#include "util/Profiler.hpp"

void HDDWiper::WipingThread::operator()()
{
	_generator=std::tr1::shared_ptr<RC4StreamProducer>(new RC4StreamProducer(HDDWiper::BUFFERSIZE,HDDWiper::BLOCKSIZE));
	while(true)
	{
		Profiler time;
		_wiper._output.write(_generator->get());
		_currentspeed=static_cast<double> (HDDWiper::BLOCKSIZE) / 1024
				/ 1024 / time.getSecondsPassed();
	}
}
