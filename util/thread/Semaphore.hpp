#pragma once

#ifndef SEMAPHORE_HPP_
#define SEMAPHORE_HPP_

#include <boost/thread/condition_variable.hpp>

/**
 * Implements a semaphore for thread synchronization.
 * A semaphore has an initial capacity.
 * Each call to wait() uses one unit of this capacity, if there is capacity available.
 * If there is no capacity available, wait() waits, until capacity gets available.
 * To the call to wait() there has to follow a call to release(), which releases one unit
 * of capacity (so the next thread can enter the semaphore).
 *
 * @author Sebastian Meßmer
 */
class Semaphore
{
public:
	/**
	 * Create a new semaphore
	 *
	 * @param capacity The initial capacity of the semaphore
	 */
	Semaphore(int capacity);

	/**
	 * Use one capacity unit. If none is available, wait for it.
	 */
	void wait();

	/**
	 * Free one used capacity unit.
	 */
	void release();
private:
	//Forbid copying
	Semaphore(const Semaphore &rhs);
	Semaphore &operator=(const Semaphore &rhs);

	boost::mutex _mutex;
	boost::condition_variable _wait;
	int _value;
};

#include "impl/Semaphore.impl.hpp"

#endif /* SEMAPHORE_HPP_ */
