inline Semaphore::Semaphore(int value)
	:_mutex(),_wait(),_value(value)
{
}

inline void Semaphore::wait()
{
	std::unique_lock<std::mutex> lock(_mutex);

	--_value;

	if(_value<0)
		_wait.wait(lock);
}

inline void Semaphore::release()
{
	std::lock_guard<std::mutex> lock(_mutex);

	++_value;

	if(_value<=0)
		_wait.notify_one();
}
