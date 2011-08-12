#include "../InputFile.hpp"

int InputFile::read(Data &data)
{
	size_t read=fread(data.get(), 1, data.size(), _file);
	if((read!=data.size()) && (!feof(_file)))
		throw std::runtime_error("Error reading from input file. ferror: " + IntToStr(ferror(_file)));

	_bytes_read=_bytes_read+read;

	return read;
}
