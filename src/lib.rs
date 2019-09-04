//! # nblistener
//! nblistener provides the [Listener](trait.Listener.html) trait to simplify
//! interactions with [TcpListener](https://doc.rust-lang.org/nightly/std/net/struct.TcpListener.html).
//!
//! ---
//!
//! std::net::TcpListener does not provide a simple interface to stop
//! handling connections once the incoming() method is invoked.
//!
//! [Listener](trait.Listener.html) provides enough support to allow a
//! listening socket to be closed from another thread in a fairly
//! re-active fashion.
//!
//! It does this by wrapping a non-blocking TcpListener which sleeps for
//! a user specified duration (10ms is a good choice) when the listener
//! would otherwise block.
//!
//! This is not the highest performance or most efficient way to solve this
//! kind of problem, but the interface is fairly ergonomic and may help out
//! anyone who is struggling to use TcpListener and wants something simple
//! to support testing or low throughput usage.
//!

use libc;
use std::io::{Error, ErrorKind};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

const EBADF: i32 = 9;

/// Closeable Listener
///
/// # Examples
/// ```rust
/// use std::net::{TcpListener, TcpStream};
/// use std::sync::Arc;
/// use std::thread;
/// use std::time::Duration;
/// use nblistener::Listener;
///
/// // Handle our client request
/// fn handle_client(_stream: TcpStream) {
///     println!("handled");
/// }
///
/// fn main() {
///
///     // Wrap our listener in an Arc to make it easy to share
///     let listener: Arc<TcpListener> = match Listener::bind("127.0.0.1:0") {
///         Ok(l) => Arc::new(l),
///         Err(err) => panic!("Cannot bind: {}", err),
///     };
///
///     // Clone our listener for sharing with our control thread
///     let l_clone = listener.clone();
///
///     // Spawn a control thread. In this example, just wait 5 seconds and
///     // then close down our listener. In real life, do whatever...
///     thread::spawn(move || {
///         thread::sleep(Duration::from_secs(5));
///         l_clone.close();
///     });
///
///     // Start handling incoming connections to our listener.
///     // If the listener would block, i.e.: no incoming connections to process,
///     // then this thread will sleep for 10ms. Each handled connection will call
///     // handle_client() for user specified connection handling.
///     match listener.handle_incoming(handle_client, Duration::from_millis(10)) {
///         Ok(_) => (),
///         Err(err) => println!("Terminated with: {}", err),
///     }
/// }
/// ```

pub trait Listener {
    /// Creates a new TcpListener which will be bound to the specified
    /// address. Works exactly the same as TcpListener::bind(), but
    /// always forces the bound socket to be non-blocking.
    fn bind<A: ToSocketAddrs>(addr: A) -> Result<Self, Error>
    where
        Self: std::marker::Sized;

    /// Close the listener. No more connections will be accepted and
    /// if handle_incoming() is active, it will terminate normally.
    fn close(&self);

    /// Start handling incoming connections. On error this will
    /// terminate with an error code, unless the error is EBADF, this
    /// is interpreted as normal termination triggered by invocation
    /// of the close() method.
    fn handle_incoming(&self, handler: fn(TcpStream), timeout: Duration) -> Result<(), Error>;
}

impl Listener for TcpListener {
    fn bind<A: ToSocketAddrs>(addr: A) -> Result<Self, Error> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;

        Ok(listener)
    }

    fn close(&self) {
        unsafe {
            libc::close(self.as_raw_fd());
        }
    }

    fn handle_incoming(&self, handler: fn(TcpStream), timeout: Duration) -> Result<(), Error> {
        for stream in self.incoming() {
            match stream {
                Ok(stream) => handler(stream),
                Err(err) => {
                    if err.kind() == ErrorKind::WouldBlock {
                        thread::sleep(timeout);
                    } else {
                        if let Some(val) = err.raw_os_error() {
                            if val == EBADF {
                                return Ok(());
                            }
                        }
                        return Err(err);
                    }
                }
            }
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Handle our client request
    fn handle_client(_stream: TcpStream) {
        println!("handled");
    }

    #[test]
    fn test_normal() {
        let listener: Arc<TcpListener> = match Listener::bind("127.0.0.1:0") {
            Ok(l) => Arc::new(l),
            Err(err) => panic!("Cannot bind: {}", err),
        };
        let l_clone = listener.clone();

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(5));
            l_clone.close();
        });

        match listener.handle_incoming(handle_client, Duration::from_millis(10)) {
            Ok(_) => (),
            Err(err) => println!("Terminated with: {}", err),
        }
    }

    #[test]
    fn test_pre_close() {
        let listener: Arc<TcpListener> = match Listener::bind("127.0.0.1:0") {
            Ok(l) => Arc::new(l),
            Err(err) => panic!("Cannot bind: {}", err),
        };
        let l_clone = listener.clone();

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(5));
            l_clone.close();
        });

        listener.close();

        match listener.handle_incoming(handle_client, Duration::from_millis(10)) {
            Ok(_) => (),
            Err(err) => println!("Terminated with: {}", err),
        }
    }
}
