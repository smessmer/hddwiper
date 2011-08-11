inline Semaphore::Semaphore(int value)
	:_mutex(),_wait(),_value(value)
{
}

inline void Semaphore::wait()
{
	boost::unique_lock<boost::mutex> lock(_mutex);

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
