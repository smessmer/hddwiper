template<class T>
inline ThreadsafeQueue<T>::ThreadsafeQueue()
	:_queue(),_mutex()
{
}

template<class T>
inline void ThreadsafeQueue<T>::push(T object)
{
	std::lock_guard<std::mutex> lock(_mutex);

	_queue.push(std::move(object));
}

template<class T>
inline T ThreadsafeQueue<T>::pop()
{
	std::lock_guard<std::mutex> lock(_mutex);

	T result = std::move(_queue.front());
	_queue.pop();
	return result;
}

template<class T>
inline bool ThreadsafeQueue<T>::empty() const
{
	std::lock_guard<std::mutex> lock(_mutex);

	return _queue.empty();
}

template<class T>
inline unsigned int ThreadsafeQueue<T>::size() const
{
	std::lock_guard<std::mutex> lock(_mutex);

	return _queue.size();
}
