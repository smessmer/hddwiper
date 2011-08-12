#include "../Data.hpp"

#include <stdexcept>

const Data Data::getSubData(const unsigned int start, const unsigned int length) const
{
	/*
	 * To check: start+length > size.
	 * Here an overflow could occur in start+length.
	 * Written as start > size - length,
	 * there can be no over/underflow, if size>=length.
	 * So check length<=size before.
	 */
	if(length>_data->size())
		throw std::logic_error("Specified subdata larger than data itself");
	if(start>_data->size()-length)
		throw std::logic_error("Specified subdata outside of range");
	Data result(length);
	memcpy(result.get(),get()+start,length);
	return result;
}
