#include <iostream>
#include <boost/program_options.hpp>

#include "HDDWiper.hpp"

using namespace std;

namespace po=boost::program_options;

// HDDWiper

int main(int argc, char *argv[])
{
	struct Options {
		Options(): output(),skip(0) {}

		string output;
		long long int skip;
	} options;

	//General options
	po::options_description desc(string("\nSyntax: ")+argv[0]+" [Options] output-file\n\nOptions");
	desc.add_options()
	    ("help,h", "Show this message")
	    ("skip,s", po::value<long long int>(&options.skip)->default_value(0), "Number of bytes to skip at the start of the output file")
	;

	//Positional options (output file)
	po::options_description posdesc;
	posdesc.add(desc).add_options()
		("output", po::value<string>(&options.output), "File where to write the random data to")
	;
	po::positional_options_description p;
	p.add("output", 1);

	//Parse command line
	po::variables_map vm;
	po::store(po::command_line_parser(argc, argv).
	          options(posdesc).positional(p).run(), vm);
	po::notify(vm);

	if (vm.count("help") or !vm.count("output")) {
	    cout << desc << "\n";
	    return 1;
	}

	HDDWiper wiper(options.output,options.skip);

	while (wiper.isRunning())
	{
		cout.precision(2);
		cout << fixed ;
		cout << "\rWritten: " <<  setw(7) << static_cast<double>(wiper.getBytesWritten())/1024/1024/1024 << " GB"
			 << "\tSpeed: " << setw(7) << wiper.getCurrentSpeed() << " MB/s"
			 << "\tSeedbuffer: " << setw(3) << wiper.getSeedBufferSize()
			 << "\tRandombuffer: " << setw(3) << wiper.getBufferSize()
			 << "\tSeeding: "<< setw(3) << wiper.getSeedingStatus()
			 << flush;
		boost::this_thread::sleep(boost::posix_time::seconds(1));
	}

	cout << "\n";

	return 0;
}


//HDDTester
/*
#include "rc4/RC4StreamProducer.hpp"
#include "kernelentropy/KernelEntropyProducer.hpp"
#include "file/OutputFile.hpp"
#include "file/InputFile.hpp"
#include <fstream>
int main(int argc, char *argv[])
{
   cout << "Seeding..."<<flush;
   KernelEntropyProducer entropyprod(1,256);
   while(entropyprod.available_count()<1)
   {
     cout << "\rSeeding..."<<entropyprod.seeding_status()<<"/256"<<flush;
     sleep(1);
   }
   Data entropy=entropyprod.get();
   //TODO Remove seedfile output (?)
   {
	   OutputFile seedfile("used.seed");
	   if((int)entropy.size()!=seedfile.write(entropy))
		   cout << "Couldn't write full entropy to disk" << endl;
   }
   cout << "\rSeeding...finished"<<endl;
   RC4StreamProducer generator(3,100*1024*1024,entropy);
   //Write
   int lastblock_written;
   unsigned long long int sum_written;
   {
    OutputFile out(argv[1]);
    while(true)
    {
      Data random = generator.get();
      lastblock_written=out.write(random);
      if(lastblock_written!=(int)random.size())
    	  break;
      cout << "\rWritten "<<out.getBytesWritten() << " bytes"<<flush;
    }
    cout << "\rWrite finished: "<<out.getBytesWritten() << " bytes"<<endl;
    sum_written=out.getBytesWritten();
   }
   
   //Check
   int lastblock_read;
   unsigned long long int sum_read;
   RC4StreamProducer generator2(3,100*1024*1024,entropy);
   {
     InputFile in(argv[1]);
     Data read(100*1024*1024);
     while(true)
     {
       Data random=generator2.get();
       lastblock_read=in.read(read);
       if(lastblock_read!=(int)random.size())
       {
    	   //Check last (partly available) block
    	   if(lastblock_written!=lastblock_read)
    		   cerr << "Difference in size of the last block!"<<endl;
    	   if(random.getSubData(0,lastblock_read)!=read.getSubData(0,lastblock_read))
    		   cerr << "Read values differ from written values in last block"<<endl;
    	   break;
       }
       if(random!=read)
    	   cerr << "\nRead value differs from written value: " << std::endl;
       cout << "\rRead "<<in.getBytesRead() << " bytes"<<flush;
     }
     cout << "\rRead finished: "<<in.getBytesRead() << " bytes"<<endl;
     sum_read=in.getBytesRead();
   }
   
   if(sum_read!=sum_written)
	   cerr << "Written byte count differs from read byte count"<<endl;

}
*/
