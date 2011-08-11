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
