#pragma once

#ifndef __RC4STREAMGENERATOR_HPP__
#define __RC4STREAMGENERATOR_HPP__

#include <openssl/rc4.h>
#include <boost/function.hpp>

#include "Data.hpp"
#include "DummyCallback.hpp"
#include "KernelEntropy.hpp"

class RC4Streamgenerator
{
public:
  const static unsigned int BLOCKSIZE=104857600;
  
  RC4Streamgenerator();
  
  const Data getRandomBytes();
  const void getRandomBytes(Data &data);
  
  void reseed(boost::function<void (unsigned int)> callback=DummyCallback());
private:
  Data _zeroes;
  RC4_KEY _key;
};

inline RC4Streamgenerator::RC4Streamgenerator()
  :_zeroes(BLOCKSIZE),_key()
{
  memset(_zeroes.get(),0,BLOCKSIZE);
  reseed();
}

inline void RC4Streamgenerator::reseed(boost::function<void (unsigned int)> callback)
{
  const unsigned int SEEDSIZE=256;
  Data seeddata=KernelEntropy::getEntropy(SEEDSIZE,callback);
  RC4_set_key(&_key,SEEDSIZE,seeddata.get());
}

inline const Data RC4Streamgenerator::getRandomBytes()
{
  Data result(BLOCKSIZE);
  getRandomBytes(result);
  return result;
}

inline const void RC4Streamgenerator::getRandomBytes(Data &data)
{
  RC4(&_key,BLOCKSIZE,_zeroes.get(),data.get());
}

#endif
