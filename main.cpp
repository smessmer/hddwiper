#include <iostream>
#include <boost/program_options.hpp>
#include <boost/lexical_cast.hpp>

#include "HDDWiper.hpp"

using namespace std;

namespace po=boost::program_options;

long long int StringToBytecount(string toParse)
{
	long long int result=1;
	switch(toParse[toParse.size()-1])
	{
	case 'T':
		result*=1024;
	case 'G':
		result*=1024;
	case 'M':
		result*=1024;
	case 'K':
		result*=1024;
	case 'B':
		toParse=toParse.substr(0,toParse.size()-1);
	default:
		result*=boost::lexical_cast<long double>(toParse);
	}
	return result;
}

// HDDWiper

int main(int argc, char *argv[])
{
	struct Options {
		Options(): output(),skip(0),blocksize(0),buffersize(0), blocks_per_seed(0) {}

		string output;
		long long int skip;
		long long int blocksize;
		unsigned int buffersize;
		unsigned int blocks_per_seed;
                bool disable_rdrand;
	} options;

	//General options
	po::options_description desc(string("\nSyntax: ")+argv[0]+" [Options] output-file\n\nOptions");
	desc.add_options()
	    ("help,h", "Show this message")
	    ("skip,s", po::value<string>()->default_value("0"), "Number of bytes to skip at the start of the output file.\nYou can either give the amount in bytes or use one of the postfixes K,M,G,T, when you want to use the corresponding power of 1024.\nYou can use floating point values (for example 3.4K).")
	    ("blocksize,b", po::value<string>()->default_value("100M"), "Size of the random blocks that are generated, stored in memory, and in one rush written to the disk. For the allowed option syntax see the skip option (example: --blocksize=10.2M)")
	    ("buffersize,u", po::value<unsigned int>(&options.buffersize)->default_value(5), "Maximum number of random blocks (see --blocksize for the size of the blocks) to produce beforehand and keep in memory (Hard disk is usually slower than the random generator)")
            ("blocks_per_seed,z", po::value<unsigned int>(&options.blocks_per_seed)->default_value(100), "Number of random blocks to create with the same seed")
            ("disable-rdrand", "Disable RDRAND generator. By default, if the CPU has RDRAND, then hddwiper uses both, a software random generator and RDRAND, and xors the streams together for better randomness. But on older CPUs, RDRAND is slow. With this option, hddwiper will only use the software generator.")
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

	//Parse arguments
	options.skip=StringToBytecount(vm["skip"].as<string>());
	if(options.skip<0)
	{
		cerr << "Negative --skip value isn't allowed\n";
		return EXIT_FAILURE;
	}
	options.blocksize=StringToBytecount(vm["blocksize"].as<string>());
	if(options.blocksize<0)
	{
		cerr << "Negative --blocksize value isn't allowed\n";
		return EXIT_FAILURE;
	}
        options.disable_rdrand=vm.count("disable-rdrand");
        if (options.disable_rdrand) {
          std::cout << "Running without RDRAND" << std::endl;
        }

	if (vm.count("help") or !vm.count("output")) {
	    cout << desc << "\n";
	    return 1;
	}

	HDDWiper wiper(options.output,options.skip,options.blocksize,options.buffersize, options.blocks_per_seed, options.disable_rdrand);

	while (wiper.isRunning())
	{
		cout.precision(2);
		cout << fixed ;
		cout << "\rWritten: " <<  setw(7) << static_cast<double>(wiper.getBytesWritten())/1024/1024/1024 << " GB"
			 << "\tSpeed: " << setw(7) << wiper.getCurrentSpeed() << " MB/s"
			 << "\tSeedbuffer: " << setw(3) << wiper.getSeedBufferSize()
			 << "\tRandombuffer: " << setw(3) << wiper.getBufferSize()
			 << "\tSeeding: "<< setw(3) << static_cast<int>(std::round(100*wiper.getSeedingStatus())) << "%"
			 << flush;
		boost::this_thread::sleep(boost::posix_time::seconds(1));
	}

	cout << "\nfinished" << std::endl;

	return 0;
}


//HDDTester
/*
#include "randomstream/RandomStreamProducer.hpp"
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
   RandomStreamProducer generator(3,100*1024*1024,entropy);
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
   RandomStreamProducer generator2(3,100*1024*1024,entropy);
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
