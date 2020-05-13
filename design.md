
client is responsible for
    + doing user interaction
    + display text/system state
    + parsing commands
    - anything that needs low-latency should happen on the client

server is responsible for
    + dealing with the file system
    + being the source of truth for buffers
    + interfacing with LSP, shells, etc

client/server sync their text buffers by interchanging the piece table change log data

on startup, the client should start a server for the local machine, but clients should be able to connect to any number of servers on other remote machines 

## things to have
+ autosave


# RPC

## Types

+ BufferId - usize
    - unique ID for a buffer on the server, scoped to that server instance
+ BufferContents - string
+ ChangeLog - a list of `pk_common::piece_table::Action` objects describing changes since the last sync

## Messages

- connect
- open buffer `(path) -> { BufferId, BufferContents }`
    + opens a buffer on the server, or returns the existing ID if it has already been loaded
- sync buffer `(BufferId, ChangeLog) -> ()`
    + syncs the server buffer with changes from the client
- reload buffer from file system `(BufferId) -> { BufferContents }`
    + reload data from server file system, return new contents
- close buffer `(BufferId) -> ()`

- list files in directory `(path) -> [DirEntry]`


# Goals
- Modal editing
- Language Server support
- integrated terminal support
- Low latency, no freezing, no jank
- usable in the near future
- some sort of remote ability that is smooth even on less-than-ideal network connection
    - less-than-ideal = DSL broadband
