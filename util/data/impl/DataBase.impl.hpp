#include <cstdlib>
#include <cstring>

inline bool operator==(const DataBase &lhs, const DataBase &rhs)
{
  if(lhs.size()!=rhs.size())
    return false;

  return 0==memcmp(lhs.get(),rhs.get(),lhs.size());
}

inline bool operator!=(const DataBase &lhs, const DataBase &rhs)
{
	return !operator==(lhs,rhs);
}

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
