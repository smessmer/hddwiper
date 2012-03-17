#ifndef HDDWIPER_HPP_
#define HDDWIPER_HPP_

#include <string>
#include <boost/thread.hpp>

#include "rc4/RC4StreamProducerAutoseed.hpp"
#include "file/OutputFile.hpp"
#include "util/thread/Threadsafe.hpp"

class HDDWiper
{
public:
	static const unsigned int BUFFERSIZE=5; //in blocks

	HDDWiper(const std::string &filename, const long long int skip=0, const long long int blocksize=100*1024*1024);

	double getCurrentSpeed() const;

	unsigned long long int getBytesWritten() const;

	unsigned int getBufferSize() const;
	unsigned int getSeedBufferSize() const;

	unsigned int getSeedingStatus() const;

	bool isRunning() const;

	long long int getBlocksize() const;
private:
	class WipingThread
	{
	public:
		WipingThread(HDDWiper &wiper);

		void operator()();

		unsigned int getBufferSize() const;
		unsigned int getSeedBufferSize() const;
		double getCurrentSpeed() const;
		unsigned int getSeedingStatus() const;
		bool isRunning() const;
	private:
		WipingThread(const WipingThread &rhs);
		WipingThread &operator=(const WipingThread &rhs);

		HDDWiper &_wiper;
		std::tr1::shared_ptr<RC4StreamProducerAutoseed> _generator;
		Threadsafe<double> _currentspeed;
		Threadsafe<bool> _is_running;
	};

	long long int _blocksize;

	OutputFile _output;

	WipingThread _wipingthread;
	boost::thread _wipingthread_thread;
};

inline HDDWiper::WipingThread::WipingThread(HDDWiper &wiper)
	:_wiper(wiper),_generator(),_currentspeed(0),_is_running(true)
{
}

inline double HDDWiper::WipingThread::getCurrentSpeed() const
{
	return _currentspeed;
}

inline HDDWiper::HDDWiper(const std::string &filename, const long long int skip, const long long int blocksize)
	:_blocksize(blocksize),_output(filename),_wipingthread(*this),_wipingthread_thread(boost::ref(_wipingthread))
{
	if(skip>0)
		_output.skip(skip);
}

inline long long int HDDWiper::getBlocksize() const
{
	return _blocksize;
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
	if(_generator==NULL)
		return 0;

	return _generator->available_count();
}

inline unsigned int HDDWiper::WipingThread::getSeedBufferSize() const
{
	if(_generator==NULL)
		return 0;

	return _generator->available_seed();
}

inline unsigned int HDDWiper::getBufferSize() const
{
	return _wipingthread.getBufferSize();
}

inline unsigned int HDDWiper::getSeedBufferSize() const
{
	return _wipingthread.getSeedBufferSize();
}

inline unsigned int HDDWiper::getSeedingStatus() const
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

inline unsigned int HDDWiper::WipingThread::getSeedingStatus() const
{
	if(_generator==NULL)
		return 0;

	return _generator->seeding_status();
}

#endif /* HDDWIPER_HPP_ */
