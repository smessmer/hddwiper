#ifndef HDDWIPER_HPP_
#define HDDWIPER_HPP_

#include <string>
#include <boost/thread.hpp>
#include <randomstream/RandomStreamProducerAutoseed.hpp>
#include "file/OutputFile.hpp"
#include "util/thread/Threadsafe.hpp"

class HDDWiper
{
public:

	HDDWiper(const std::string &filename, const unsigned long long int skip=0, const unsigned long long int blocksize=100*1024*1024, const unsigned int buffersize=5, const unsigned int blocks_per_seed=10);

	double getCurrentSpeed() const;

	unsigned long long int getBytesWritten() const;

	unsigned int getBufferSize() const;
	unsigned int getSeedBufferSize() const;
	unsigned int getBlocksPerSeed() const;

	double getSeedingStatus() const;

	bool isRunning() const;

	unsigned long long int getBlocksize() const;
	unsigned int getBuffersize() const;
private:
	class WipingThread
	{
	public:
		WipingThread(HDDWiper &wiper);

		void operator()();

		unsigned int getBufferSize() const;
		unsigned int getSeedBufferSize() const;
		double getCurrentSpeed() const;
		double getSeedingStatus() const;
		bool isRunning() const;
	private:
		WipingThread(const WipingThread &rhs);
		WipingThread &operator=(const WipingThread &rhs);

		HDDWiper &_wiper;
		std::unique_ptr<KernelEntropyProducer> _kernel_entropy_producer;
		std::unique_ptr<Assembly<Data>> _kernel_entropy_assembly;
		std::vector<std::unique_ptr<RandomStreamProducerAutoseed>> _random_block_producers;
		std::unique_ptr<Assembly<Data>> _random_block_assembly;
		Threadsafe<double> _currentspeed;
		Threadsafe<bool> _is_running;
	};


	static const unsigned int SEEDCOUNT=200;

	//Size of one block of generated random data
	unsigned long long int _blocksize;
	//Maximum number of random blocks to store in the generating queue (producer/consumer pattern)
	unsigned int _buffersize;
	//Number of random blocks to create with one seed
    unsigned int _blocks_per_seed;

	OutputFile _output;

	WipingThread _wipingthread;
	boost::thread _wipingthread_thread;
};

inline HDDWiper::WipingThread::WipingThread(HDDWiper &wiper)
	:_wiper(wiper),_kernel_entropy_assembly(), _random_block_assembly()
	, _random_block_producers(),_currentspeed(0),_is_running(true)
{
}

inline double HDDWiper::WipingThread::getCurrentSpeed() const
{
	return _currentspeed;
}

inline HDDWiper::HDDWiper(const std::string &filename, const unsigned long long int skip, const unsigned long long int blocksize, const unsigned int buffersize, const unsigned int blocks_per_seed)
	:_blocksize(blocksize),_buffersize(buffersize),_blocks_per_seed(blocks_per_seed),_output(filename),_wipingthread(*this),_wipingthread_thread(boost::ref(_wipingthread))
{
	if(skip>0)
		_output.skip(skip);
}

inline unsigned long long int HDDWiper::getBlocksize() const
{
	return _blocksize;
}

inline unsigned int HDDWiper::getBuffersize() const
{
	return _buffersize;
}

inline unsigned int HDDWiper::getBlocksPerSeed() const
{
	return _blocks_per_seed;
}

inline double HDDWiper::getCurrentSpeed() const
{
	return _wipingthread.getCurrentSpeed();
}

inline unsigned long long int HDDWiper::getBytesWritten() const
{
	return _output.getBytesWritten();
}

inline unsigned int HDDWiper::WipingThread::getBufferSize() const
{
	if(_random_block_producers.size() == 0)
		return 0;

	return _random_block_assembly->size();
}

inline unsigned int HDDWiper::WipingThread::getSeedBufferSize() const
{
	if(_random_block_producers.size() == 0)
		return 0;

	return _kernel_entropy_assembly->size();
}

inline unsigned int HDDWiper::getBufferSize() const
{
	return _wipingthread.getBufferSize();
}

inline unsigned int HDDWiper::getSeedBufferSize() const
{
	return _wipingthread.getSeedBufferSize();
}

inline double HDDWiper::getSeedingStatus() const
{
	return _wipingthread.getSeedingStatus();
}

inline bool HDDWiper::isRunning() const
{
	return _wipingthread.isRunning();
}

inline bool HDDWiper::WipingThread::isRunning() const
{
	return _is_running;
}

inline double HDDWiper::WipingThread::getSeedingStatus() const
{
	if(_random_block_producers.size() == 0)
		return 0;

  return static_cast<double>(_kernel_entropy_producer->seeding_status()) / RandomStreamGenerator::SeedSize();
}

#endif /* HDDWIPER_HPP_ */
