#pragma once

#ifndef __DATA_HPP__
#define __DATA_HPP__

#include <tr1/memory>

#include "DataBase.hpp"

class Data
{
public:
  Data(const unsigned int size);
  
  unsigned char *get();
  const unsigned char *get() const;
  
  unsigned int size() const;
private:
  std::tr1::shared_ptr<DataBase> _data;
};

inline Data::Data(const unsigned int size)
  :_data(new DataBase(size))
{
}

inline unsigned char *Data::get()
{
  return const_cast<unsigned char*>(const_cast<const Data*>(this)->get());
}

inline const unsigned char *Data::get() const
{
  return _data->get();
}

inline unsigned int Data::size() const
{
  return _data->size();
}

#endif
