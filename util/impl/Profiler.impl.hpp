inline Profiler::Profiler() :
	_start(currentTime())
{
}

inline double Profiler::getSecondsPassed() const
{
	return static_cast<double> (currentTime() - _start) / 1000000;
}

inline unsigned long long int Profiler::currentTime() const
{
	struct timeval tm;
	gettimeofday(&tm, 0);
	return static_cast<unsigned long long int> (tm.tv_sec) * 1000000
			+ tm.tv_usec;
}
