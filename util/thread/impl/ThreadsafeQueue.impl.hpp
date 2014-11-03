template<class T>
inline ThreadsafeQueue<T>::ThreadsafeQueue()
	:_queue(),_mutex()
{
}

template<class T>
inline void ThreadsafeQueue<T>::push(const T &object)
{
	std::lock_guard<std::mutex> lock(_mutex);

	_queue.push(object);
}

template<class T>
inline const T ThreadsafeQueue<T>::pop()
{
	std::lock_guard<std::mutex> lock(_mutex);

	const T result=_queue.front();
	_queue.pop();
	return result;
}

template<class T>
inline const T ThreadsafeQueue<T>::top()
{
	std::lock_guard<std::mutex> lock(_mutex);

	return _queue.front();
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
