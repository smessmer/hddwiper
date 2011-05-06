#pragma once

#ifndef __DATABASE_HPP__
#define __DATABASE_HPP__

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

inline DataBase::DataBase(const unsigned int size) :
	_data(new unsigned char[size]), _size(size)
{
}

inline DataBase::~DataBase()
{
	delete[] _data;
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
