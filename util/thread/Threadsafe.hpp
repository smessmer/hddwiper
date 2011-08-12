#pragma once

#ifndef THREADSAFE_HPP_
#define THREADSAFE_HPP_

#include <boost/thread/mutex.hpp>

/**
 * This class can be used to make a value threadsafe.
 * It has getters and setters, that use locks to disallow concurrent accesses.
 * Warning: If you first get a value and then set it, it is possible,
 * that another thread changed it in the time inbetween.
 * So for increment operations, use Threadsafe<T>::operator++ or
 * Threadsafe<T>::operator--.
 *
 * For example Threadsafe<int> is a threadsafe integer. Instances
 * of Threadsafe<int> can be used like int itself, because
 * Threadsafe<int> offers operator++, operator--, operator+=, ...
 *
 * @tparam <T> The type of objects to store.
 *
 * @author Sebastian Meßmer
 */
template<class T>
class Threadsafe
{
public:
	/**
	 * Create a new threadsafe object
	 * @param object The initial value
	 */
	Threadsafe(const T &object);

	/**
	 * Set the value of the stored object
	 *
	 * @param object The new value
	 */
	void set(const T &object);

	/**
	 * Return (a copy of) the current value of the stored object.
	 * This function returns a copy, so following changes by other
	 * threads aren't a problem.
	 *
	 * @return The current value
	 */
	const T get() const;

	/**
	 * Copy a threadsafe object.
	 *
	 * @param rhs The source where to copy from
	 */
	Threadsafe(const Threadsafe<T> &rhs);

	/**
	 * Assignment operator - assign from another threadsafe object
	 * @param rhs The object to assign from
	 * @return Reference to *this
	 */
	Threadsafe<T> &operator=(const Threadsafe<T> &rhs);

	/**
	 * Return (a copy of) the current stored value
	 *
	 * @return The current stored value
	 */
	operator const T() const;

	/**
	 * Increment the stored value and return the new value
	 * Class T must support the prefix operator ++.
	 *
	 * @return Reference to *this
	 */
	Threadsafe<T> &operator++();

	/**
	 * Increment the stored value and return the old value
	 * Class T must support the prefix (!) operator ++.
	 *
	 * @return The old value of the object
	 */
	const Threadsafe<T> operator++(int);

	/**
	 * Decrement the stored value and return the new value
	 * Class T must support the prefix operator --.
	 *
	 * @return Reference to *this
	 */
	Threadsafe<T> &operator--();

	/**
	 * Decrement the stored value and return the old value
	 * Class T must support the prefix (!) operator --.
	 *
	 * @return The old value of the object
	 */
	const Threadsafe<T> operator--(int);

	/**
	 * Increment the stored value by a given value
	 *
	 * @param rhs The incremention value
	 * @return Reference to *this
	 */
	Threadsafe<T> &operator+=(const T &rhs);

	//TODO more functions
private:
	T _object;
	mutable boost::mutex _mutex;
};

#include "impl/Threadsafe.impl.hpp"

#endif /* THREADSAFE_HPP_ */
