template<class T>
inline ThreadsafeQueue<T>::ThreadsafeQueue()
	:_queue(),_mutex()
{
}

template<class T>
inline void ThreadsafeQueue<T>::push(const T &object)
{
	boost::lock_guard<boost::mutex> lock(_mutex);

	_queue.push(object);
}

template<class T>
inline const T ThreadsafeQueue<T>::pop()
{
	boost::lock_guard<boost::mutex> lock(_mutex);

	const T result=_queue.front();
	_queue.pop();
	return result;
}

template<class T>
inline const T ThreadsafeQueue<T>::top()
{
	boost::lock_guard<boost::mutex> lock(_mutex);

	return _queue.front();
}

template<class T>
inline bool ThreadsafeQueue<T>::empty() const
{
	boost::lock_guard<boost::mutex> lock(_mutex);

	return _queue.empty();
}

template<class T>
inline unsigned int ThreadsafeQueue<T>::size() const
{
	boost::lock_guard<boost::mutex> lock(_mutex);

	return _queue.size();
}
