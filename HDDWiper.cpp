#include "HDDWiper.hpp"

#include "util/Profiler.hpp"

void HDDWiper::WipingThread::operator()()
{
	_generator=std::tr1::shared_ptr<RC4StreamProducerAutoseed>(new RC4StreamProducerAutoseed(_wiper.getBuffersize(),_wiper.getBlocksize(),_wiper.getBlocksPerSeed()));
	double newspeed=0;
	while(_is_running)
	{
		Profiler time;

		//Get random data and set speed to zero while waiting. Then update the speed display to the newspeed.
		_currentspeed=0;
		Data randomdata=_generator->get();
		_currentspeed=newspeed;

		//Write random data to output
		if(randomdata.size()!=_wiper._output.write(randomdata))
		{
			_is_running=false;  //There is no space left on device
		}

		//Recalculate current speed
		newspeed=static_cast<double> (_wiper.getBlocksize()) / 1024
				/ 1024 / time.getSecondsPassed();
	}
}
