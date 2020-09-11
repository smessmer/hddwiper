#pragma once

#ifndef __THREADSAFEQUEUE_HPP__
#define __THREADSAFEQUEUE_HPP__

#include <queue>
#include <mutex>

/**
 * This class is a threadsafe queue.
 *
 * @tparam <T> The objects to store in the queue
 */
template<class T>
class ThreadsafeQueue
{
public:
	/**
	 * Create a new queue
	 */
	ThreadsafeQueue();

	/**
	 * Add an element to the back of the queue
	 * @param data The element to be pushed
	 */
	void push(T data);

	/**
	 * Delete the first element of the queue and return it.
	 * @return The (old) first element of the queue
	 */
	T pop();

	/**
	 * Check, if the queue is empty
	 * @return True, iff the queue is empty
	 */
	bool empty() const;

	/**
	 * Return the number of elements currently stored in the queue
	 * @return The number of elements currently stored in the queue
	 */
	unsigned int size() const;
private:
	std::queue<T> _queue;
	mutable std::mutex _mutex;
};

#include "impl/ThreadsafeQueue.impl.hpp"

#endif /* __THREADSAFEQUEUE_HPP__ */
