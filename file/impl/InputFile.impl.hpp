inline InputFile::InputFile(const std::string &filename)
	:AbstractFile(filename,"rb"),_bytes_read(0)
{
}

inline unsigned long long int InputFile::getBytesRead() const
{
	return _bytes_read;
}
