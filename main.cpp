#include <iostream>

#include "HDDWiper.hpp"

using namespace std;

int main(int argc, char *argv[])
{
	if (argc < 2)
	{
		cout << "Syntax: " << argv[0] << " [targetfile]" << endl;
		return 1;
	}

	HDDWiper wiper(argv[1]);

	while (true)
	{
		cout.precision(2);
		cout << fixed ;
		cout << "\rWritten: " <<  setw(5) << static_cast<double>(wiper.getBytesWritten())/1024/1024/1024 << " GB"
			 << "\tSpeed: " << setw(5) << wiper.getCurrentSpeed() << " MB/s"
			 << "\tSeedbuffer: " << setw(3) << wiper.getSeedBufferSize()
			 << "\tRandombuffer: " << setw(3) << wiper.getBufferSize()
			 << "\tSeeding: "<< setw(3) << wiper.getSeedingStatus()
			 << flush;
		boost::this_thread::sleep(boost::posix_time::seconds(1));
	}

	return 0;
}
