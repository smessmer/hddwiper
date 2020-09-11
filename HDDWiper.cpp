#include "HDDWiper.hpp"

#include "util/Profiler.hpp"

void HDDWiper::WipingThread::operator()()
{
	_kernel_entropy_assembly = std::make_unique<Assembly<Data>>(SEEDCOUNT);
	_random_block_assembly = std::make_unique<Assembly<Data>>(_wiper.getBuffersize());
	_kernel_entropy_producer = std::make_unique<KernelEntropyProducer>(_kernel_entropy_assembly.get(), RandomStreamGenerator::SeedSize());
	_generator = std::make_unique<RandomStreamProducerAutoseed>(_random_block_assembly.get(), _wiper.getBlocksize(), _wiper.getBlocksPerSeed(), _kernel_entropy_assembly.get());
	double newspeed=0;
	while(_is_running)
	{
		Profiler time;

		//Get random data and set speed to zero while waiting. Then update the speed display to the newspeed.
		_currentspeed=0;
		Data randomdata=_random_block_assembly->pop();
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
