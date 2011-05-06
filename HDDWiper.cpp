#include "HDDWiper.hpp"

#include "util/Profiler.hpp"

void HDDWiper::WipingThread::operator()()
{
	_generator=std::tr1::shared_ptr<RC4StreamProducer>(new RC4StreamProducer(HDDWiper::BUFFERSIZE,HDDWiper::BLOCKSIZE));
	double newspeed=0;
	while(true)
	{
		Profiler time;

		//Get random data and set speed to zero while waiting. Then update the speed display to the newspeed.
		_currentspeed=0;
		Data randomdata=_generator->get();
		_currentspeed=newspeed;

		//Write random data to output
		_wiper._output.write(randomdata);

		//Recalculate current speed
		newspeed=static_cast<double> (HDDWiper::BLOCKSIZE) / 1024
				/ 1024 / time.getSecondsPassed();
	}
}
