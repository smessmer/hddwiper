#ifndef SEMAPHORE_HPP_
#define SEMAPHORE_HPP_

#include <boost/thread/condition_variable.hpp>

class Semaphore
{
public:
	Semaphore(int value);

	void wait();
	void release();
private:
	Semaphore(const Semaphore &rhs);
	Semaphore &operator=(const Semaphore &rhs);

	boost::mutex _mutex;
	boost::condition_variable _wait;
	int _value;
};

inline Semaphore::Semaphore(int value)
	:_mutex(),_wait(),_value(value)
{
}

inline void Semaphore::wait()
{
	boost::unique_lock<boost::mutex> lock(_mutex);
	lock.lock();

	--_value;

	if(_value<0)
		_wait.wait(lock);
}

inline void Semaphore::release()
{
	boost::lock_guard<boost::mutex> lock(_mutex);

	++_value;

	if(_value<=0)
		_wait.notify_one();
}

#endif /* SEMAPHORE_HPP_ */
