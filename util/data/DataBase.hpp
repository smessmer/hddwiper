#pragma once

#ifndef __DATABASE_HPP__
#define __DATABASE_HPP__

#include <cstdlib>

class DataBase
{
public:
	DataBase(const unsigned int size);
	~DataBase();

	unsigned char *get();
	const unsigned char *get() const;

	unsigned int size() const;
private:
	//Forbid copying
	DataBase(const DataBase &rhs);
	DataBase &operator=(const DataBase &rhs);

	unsigned char *_data;
	unsigned int _size;
};

#include <stdexcept>
inline DataBase::DataBase(const unsigned int size) :
	_data(reinterpret_cast<unsigned char*>(malloc(size*sizeof(unsigned char)))), _size(size)
{
	if(_data==NULL)
		//TODO Correct exception
		throw std::logic_error("Couldn't allocate memory");
}

inline DataBase::~DataBase()
{
	if(_data!=NULL)
	{
		free(_data);
		_data=NULL;
	}
}

inline unsigned char *DataBase::get()
{
	return const_cast<unsigned char*> (const_cast<const DataBase*> (this)->get());
}

inline const unsigned char *DataBase::get() const
{
	return _data;
}

inline unsigned int DataBase::size() const
{
	return _size;
}

#endif
