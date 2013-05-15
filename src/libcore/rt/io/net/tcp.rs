// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use option::{Option, Some, None};
use result::{Ok, Err};
use rt::sched::local_sched::unsafe_borrow_io;
use rt::io::net::ip::IpAddr;
use rt::io::{Reader, Writer, Listener};
use rt::io::{io_error, read_error, EndOfFile};
use rt::rtio::{IoFactory,
               RtioTcpListener, RtioTcpListenerObject,
               RtioTcpStream, RtioTcpStreamObject};

pub struct TcpStream {
    rtstream: ~RtioTcpStreamObject
}

impl TcpStream {
    fn new(s: ~RtioTcpStreamObject) -> TcpStream {
        TcpStream {
            rtstream: s
        }
    }

    pub fn connect(addr: IpAddr) -> Option<TcpStream> {
        let stream = unsafe {
            rtdebug!("borrowing io to connect");
            let io = unsafe_borrow_io();
            rtdebug!("about to connect");
            (*io).tcp_connect(addr)
        };

        match stream {
            Ok(s) => {
                Some(TcpStream::new(s))
            }
            Err(ioerr) => {
                rtdebug!("failed to connect: %?", ioerr);
                io_error::cond.raise(ioerr);
                return None;
            }
        }
    }
}

impl Reader for TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Option<uint> {
        let bytes_read = self.rtstream.read(buf);
        match bytes_read {
            Ok(read) => Some(read),
            Err(ioerr) => {
                // EOF is indicated by returning None
                if ioerr.kind != EndOfFile {
                    read_error::cond.raise(ioerr);
                }
                return None;
            }
        }
    }

    fn eof(&mut self) -> bool { fail!() }
}

impl Writer for TcpStream {
    fn write(&mut self, buf: &[u8]) {
        let res = self.rtstream.write(buf);
        match res {
            Ok(_) => (),
            Err(ioerr) => {
                io_error::cond.raise(ioerr);
            }
        }
    }

    fn flush(&mut self) { fail!() }
}

pub struct TcpListener {
    rtlistener: ~RtioTcpListenerObject,
}

impl TcpListener {
    pub fn bind(addr: IpAddr) -> Option<TcpListener> {
        let listener = unsafe { (*unsafe_borrow_io()).tcp_bind(addr) };
        match listener {
            Ok(l) => {
                Some(TcpListener {
                    rtlistener: l
                })
            }
            Err(ioerr) => {
                io_error::cond.raise(ioerr);
                return None;
            }
        }
    }
}

impl Listener<TcpStream> for TcpListener {
    fn accept(&mut self) -> Option<TcpStream> {
        let rtstream = self.rtlistener.accept();
        match rtstream {
            Ok(s) => {
                Some(TcpStream::new(s))
            }
            Err(ioerr) => {
                io_error::cond.raise(ioerr);
                return None;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use int;
    use cell::Cell;
    use rt::test::*;
    use rt::io::net::ip::Ipv4;
    use rt::io::*;

    #[test] #[ignore]
    fn bind_error() {
        do run_in_newsched_task {
            let mut called = false;
            do io_error::cond.trap(|e| {
                assert!(e.kind == PermissionDenied);
                called = true;
            }).in {
                let addr = Ipv4(0, 0, 0, 0, 1);
                let listener = TcpListener::bind(addr);
                assert!(listener.is_none());
            }
            assert!(called);
        }
    }

    #[test]
    fn connect_error() {
        do run_in_newsched_task {
            let mut called = false;
            do io_error::cond.trap(|e| {
                assert!(e.kind == ConnectionRefused);
                called = true;
            }).in {
                let addr = Ipv4(0, 0, 0, 0, 1);
                let stream = TcpStream::connect(addr);
                assert!(stream.is_none());
            }
            assert!(called);
        }
    }

    #[test]
    fn smoke_test() {
        do run_in_newsched_task {
            let addr = next_test_ip4();

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                let mut stream = listener.accept();
                let mut buf = [0];
                stream.read(buf);
                assert!(buf[0] == 99);
            }

            do spawntask_immediately {
                let mut stream = TcpStream::connect(addr);
                stream.write([99]);
            }
        }
    }

    #[test]
    fn read_eof() {
        do run_in_newsched_task {
            let addr = next_test_ip4();

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                let mut stream = listener.accept();
                let mut buf = [0];
                let nread = stream.read(buf);
                assert!(nread.is_none());
            }

            do spawntask_immediately {
                let _stream = TcpStream::connect(addr);
                // Close
            }
        }
    }

    #[test]
    fn read_eof_twice() {
        do run_in_newsched_task {
            let addr = next_test_ip4();

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                let mut stream = listener.accept();
                let mut buf = [0];
                let nread = stream.read(buf);
                assert!(nread.is_none());
                let nread = stream.read(buf);
                assert!(nread.is_none());
            }

            do spawntask_immediately {
                let _stream = TcpStream::connect(addr);
                // Close
            }
        }
    }

    #[test]
    fn write_close() {
        do run_in_newsched_task {
            let addr = next_test_ip4();

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                let mut stream = listener.accept();
                let buf = [0];
                loop {
                    let mut stop = false;
                    do io_error::cond.trap(|e| {
                        // NB: ECONNRESET on linux, EPIPE on mac
                        assert!(e.kind == ConnectionReset || e.kind == BrokenPipe);
                        stop = true;
                    }).in {
                        stream.write(buf);
                    }
                    if stop { break }
                }
            }

            do spawntask_immediately {
                let _stream = TcpStream::connect(addr);
                // Close
            }
        }
    }

    #[test]
    fn multiple_connect_serial() {
        do run_in_newsched_task {
            let addr = next_test_ip4();
            let max = 10;

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                for max.times {
                    let mut stream = listener.accept();
                    let mut buf = [0];
                    stream.read(buf);
                    assert!(buf[0] == 99);
                }
            }

            do spawntask_immediately {
                for max.times {
                    let mut stream = TcpStream::connect(addr);
                    stream.write([99]);
                }
            }
        }
    }

    #[test]
    fn multiple_connect_interleaved_greedy_schedule() {
        do run_in_newsched_task {
            let addr = next_test_ip4();
            static MAX: int = 10;

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                for int::range(0, MAX) |i| {
                    let stream = Cell(listener.accept());
                    rtdebug!("accepted");
                    // Start another task to handle the connection
                    do spawntask_immediately {
                        let mut stream = stream.take();
                        let mut buf = [0];
                        stream.read(buf);
                        assert!(buf[0] == i as u8);
                        rtdebug!("read");
                    }
                }
            }

            connect(0, addr);

            fn connect(i: int, addr: IpAddr) {
                if i == MAX { return }

                do spawntask_immediately {
                    rtdebug!("connecting");
                    let mut stream = TcpStream::connect(addr);
                    // Connect again before writing
                    connect(i + 1, addr);
                    rtdebug!("writing");
                    stream.write([i as u8]);
                }
            }
        }
    }

    #[test] #[ignore(reason = "hangs on mac")]
    fn multiple_connect_interleaved_lazy_schedule() {
        do run_in_newsched_task {
            let addr = next_test_ip4();
            static MAX: int = 10;

            do spawntask_immediately {
                let mut listener = TcpListener::bind(addr);
                for int::range(0, MAX) |_| {
                    let stream = Cell(listener.accept());
                    rtdebug!("accepted");
                    // Start another task to handle the connection
                    do spawntask_later {
                        let mut stream = stream.take();
                        let mut buf = [0];
                        stream.read(buf);
                        assert!(buf[0] == 99);
                        rtdebug!("read");
                    }
                }
            }

            connect(0, addr);

            fn connect(i: int, addr: IpAddr) {
                if i == MAX { return }

                do spawntask_later {
                    rtdebug!("connecting");
                    let mut stream = TcpStream::connect(addr);
                    // Connect again before writing
                    connect(i + 1, addr);
                    rtdebug!("writing");
                    stream.write([99]);
                }
            }
        }
    }

}
