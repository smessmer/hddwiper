#include <chrono>
#include <boost/thread.hpp>

inline Semaphore::Semaphore(int value)
	:_mutex(),_wait(),_value(value)
{
}

inline void Semaphore::wait()
{
	std::unique_lock<std::mutex> lock(_mutex);

        while (_value <= 0) {
            _wait.wait_for(lock, std::chrono::milliseconds(100), [&] {
                boost::this_thread::interruption_point();
                return _value > 0;
            });
        }

	--_value;
}

inline void Semaphore::release()
{
	std::lock_guard<std::mutex> lock(_mutex);

	++_value;

	_wait.notify_one();
}
