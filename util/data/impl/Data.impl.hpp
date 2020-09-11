inline bool operator==(const Data &lhs, const Data &rhs)
{
  return *lhs._data==*rhs._data;
}

inline bool operator!=(const Data &lhs, const Data &rhs)
{
  return !operator==(lhs,rhs);
}

inline Data::Data(const unsigned int size) :
	_data(std::make_unique<DataBase>(size))
{
}

inline unsigned char *Data::get()
{
	return const_cast<unsigned char*> (const_cast<const Data*> (this)->get());
}

inline const unsigned char *Data::get() const
{
	return _data->get();
}

inline unsigned int Data::size() const
{
	return _data->size();
}
