#include "HDDWiper.hpp"

#include "util/Profiler.hpp"

void HDDWiper::WipingThread::operator()()
{
	while(true)
	{
		Profiler time;
		_wiper._output.write(_generator.get());
		_currentspeed=static_cast<double> (HDDWiper::BLOCKSIZE) / 1024
				/ 1024 / time.getSecondsPassed();
	}
}
