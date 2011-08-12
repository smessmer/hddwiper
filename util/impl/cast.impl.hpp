#ifndef CAST_IMPL_HPP_
#define CAST_IMPL_HPP_

inline const std::string IntToStr(const int value)
{
	std::ostringstream str;
	str << value;
	return str.str();
}

inline const int StrToInt(const std::string &value)
{
	return atoi(value.c_str());
}

#endif /* CAST_IMPL_HPP_ */
