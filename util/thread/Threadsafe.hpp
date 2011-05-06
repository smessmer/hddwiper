/*
 * Threadsafe.hpp
 *
 *  Created on: 06.06.2011
 *      Author: ubuntu
 */

#ifndef THREADSAFE_HPP_
#define THREADSAFE_HPP_

#include <boost/thread/mutex.hpp>

template<class T>
class Threadsafe
{
public:
	Threadsafe(const T &object);

	void set(const T &object);
	const T get() const;

	Threadsafe(const Threadsafe<T> &rhs);
	Threadsafe<T> &operator=(const Threadsafe<T> &rhs);

	operator const T() const;

	Threadsafe<T> &operator++();
	const Threadsafe<T> operator++(int);
	Threadsafe<T> &operator--();
	const Threadsafe<T> operator--(int);

	Threadsafe<T> &operator+=(const T &rhs);
	//TODO more functions
private:
	T _object;
	mutable boost::mutex _mutex;
};

template<class T>
inline Threadsafe<T>::Threadsafe(const T &object)
	:_object(object),_mutex()
{
}

template<class T>
inline Threadsafe<T>::Threadsafe(const Threadsafe<T> &rhs)
	:_object(rhs.get()),_mutex()
{
}

template<class T>
inline Threadsafe<T> &Threadsafe<T>::operator=(const Threadsafe<T> &rhs)
{
	set(rhs.get());
	return *this;
}

template<class T>
inline void Threadsafe<T>::set(const T &object)
{
	boost::lock_guard<boost::mutex> lock(_mutex);
	_object=object;
}

template<class T>
inline const T Threadsafe<T>::get() const
{
	boost::lock_guard<boost::mutex> lock(_mutex);
	return _object;
}

template<class T>
inline Threadsafe<T>::operator const T() const
{
	return get();
}

template<class T>
inline Threadsafe<T> &Threadsafe<T>::operator+=(const T &rhs)
{
	boost::lock_guard<boost::mutex> lock(_mutex);
	_object+=rhs;
	return *this;
}

template<class T>
inline Threadsafe<T> &Threadsafe<T>::operator++()
{
	boost::lock_guard<boost::mutex> lock(_mutex);
	++_object;
	return *this;
}

template<class T>
inline const Threadsafe<T> Threadsafe<T>::operator++(int)
{
	Threadsafe<T> tmp(*this);
	++*this;
	return tmp;
}

template<class T>
inline Threadsafe<T> &Threadsafe<T>::operator--()
{
	boost::lock_guard<boost::mutex> lock(_mutex);
	--_object;
	return *this;
}

template<class T>
inline const Threadsafe<T> Threadsafe<T>::operator--(int)
{
	Threadsafe<T> tmp(*this);
	--*this;
	return tmp;
}

#endif /* THREADSAFE_HPP_ */
