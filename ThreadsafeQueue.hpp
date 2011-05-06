#pragma once

#ifndef __THREADSAFEQUEUE_HPP__
#define __THREADSAFEQUEUE_HPP__

#include <queue>
#include <boost/thread/mutex.hpp>

template<class T>
class ThreadsafeQueue
{
public:
	ThreadsafeQueue();

	void push(const T &data);
	const T pop();

	bool empty() const;
private:
	std::queue<T> _queue;
	boost::mutex _mutex;
};

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
inline bool ThreadsafeQueue<T>::empty() const
{
	return _queue.empty();
}

#endif /* __THREADSAFEQUEUE_HPP__ */
