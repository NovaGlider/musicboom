# Toy Vibration to Music

This is a tool that sets the vibration via Intiface from audio input. Currently
the device has to have two motors and the host has to provide a Jack interface
(e.g. via Jack directly or the interface of Pipewire).

The first motor is following the output of a low pass filter, the second on a
high pass one.

## Usage

```
Usage: musicboom [OPTIONS] [FILTER]

Arguments:
  [FILTER]  connect to all Jack ports with this in their name [default: output]

Options:
  -d, --debug        show debug output
  -l, --low <LOW>    frequency in Hz for low pass [default: 200.0]
  -f, --high <HIGH>  frequency in Hz for high pass [default: 1000.0]
  -a, --amp <AMP>    linear amplification for vibration [default: 1.1]
  -u, --uri <URI>    URI to Intiface [default: ws://localhost:12345/ws]
  -h, --help         Print help
```