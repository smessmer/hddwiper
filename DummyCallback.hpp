#pragma once

#ifndef __DUMMYCALLBACK_HPP__
#define __DUMMYCALLBACK_HPP__

class DummyCallback
{
public:
	void operator()(unsigned int);
};

inline void DummyCallback::operator()(unsigned int)
{
}

#endif
