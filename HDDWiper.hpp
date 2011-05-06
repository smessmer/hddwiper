#ifndef HDDWIPER_HPP_
#define HDDWIPER_HPP_

#include <string>
#include <boost/thread.hpp>

#include "rc4/RC4StreamProducer.hpp"
#include "output/OutputFile.hpp"
#include "util/thread/Threadsafe.hpp"

class HDDWiper
{
public:
	static const unsigned int BLOCKSIZE=100*1024*1024;
	static const unsigned int BUFFERSIZE=5; //in blocks

	HDDWiper(const std::string &filename);

	double getCurrentSpeed() const;

	unsigned long long int getBytesWritten() const;

	unsigned int getBufferSize() const;
	unsigned int getSeedBufferSize() const;

	unsigned int getSeedingStatus() const;
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
	private:
		WipingThread(const WipingThread &rhs);
		WipingThread &operator=(const WipingThread &rhs);

		HDDWiper &_wiper;
		std::tr1::shared_ptr<RC4StreamProducer> _generator;
		Threadsafe<double> _currentspeed;
	};

	Outputfile _output;

	WipingThread _wipingthread;
	boost::thread _wipingthread_thread;
};

inline HDDWiper::WipingThread::WipingThread(HDDWiper &wiper)
	:_wiper(wiper),_generator(),_currentspeed(0)
{
}

inline double HDDWiper::WipingThread::getCurrentSpeed() const
{
	return _currentspeed;
}

inline HDDWiper::HDDWiper(const std::string &filename)
	:_output(filename),_wipingthread(*this),_wipingthread_thread(boost::ref(_wipingthread))
{
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

inline unsigned int HDDWiper::WipingThread::getSeedingStatus() const
{
	if(_generator==NULL)
		return 0;

	return _generator->seeding_status();
}

#endif /* HDDWIPER_HPP_ */
