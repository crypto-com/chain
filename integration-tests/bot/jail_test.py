#!/usr/bin/python3
import docker
import json
import requests
import datetime
import time
import jsonrpcclient
from chainrpc import RPC, Blockchain
from decouple import config
CURRENT_HASH = config('CURRENT_HASH', '')
class Program :
    def __init__(self) :
        self.rpc = RPC()
        self.blockchain = Blockchain()
        # wallet a
        self.node0_address = ""
        self.node0_mnemonics= ""

        # wallet b
        self.node1_address = ""
        self.node1_mnemonics=""

        # wallet b
        self.node2_address = ""
        self.node2_mnemonics=""
        
        self.keya=""
        self.keyb=""
        self.keyc=""


        self.headers = {
            'Content-Type': 'application/json',
        }

    def get_containers(self) :
        client = docker.from_env()
        containers= client.containers.list()
        ret= {}
        for container in containers:
            id = container
            #ret[id.name]= id.id
            ret[id.name]= container
        return ret
        

    #show_containers()
    # tendermint rpc



    def get_staking_state(self,name, enckey, addr):
        return self.rpc.staking.state(addr, name, enckey)
       

    def create_staking_address(self,name, enckey):
        return self.rpc.address.create(name,'staking', enckey)
       
    def restore_wallets(self):
        print("restore wallets")
        self.rpc.wallet.restore(self.node0_mnemonics, "a")
        self.rpc.wallet.restore(self.node1_mnemonics, "b")
        self.rpc.wallet.restore(self.node2_mnemonics, "c")
            

    def create_addresses(self):
        self.create_staking_address("a", self.keya)
        self.create_staking_address("a", self.keya)
        self.create_staking_address("b", self.keyb)
        self.create_staking_address("b", self.keyb)
        self.create_staking_address("c", self.keyc)
        self.create_staking_address("c", self.keyc)
        

    def unjail(self,name, enckey, address):
        try:
            return self.rpc.staking.unjail(address, name, enckey)
        except jsonrpcclient.exceptions.JsonRpcClientError as ex:
            print("unjail fail={}".format(ex))

    def check_validators(self) :
        try: 
            x= self.rpc.chain.validators() 
            print(x)
            data =len(x["validators"])
            return data
        except requests.ConnectionError:
            return 0

    def check_validators_old(self) :
        x=self.blockchain.validators()["validators"]
        print("check validators")
        data =len(x)
        print("count={}  check_validators={}".format(data,x))
        return data
      

    def wait_for_ready(self,count) :
        initial_time=time.time() # in seconds
        MAX_TIME = 3600
        while True:
            current_time= time.time()
            elasped_time= current_time - initial_time
            remain_time = MAX_TIME - elasped_time
            validators=self.check_validators()
            if remain_time< 0 :
                assert False
            print("{0}  remain time={1:.2f}  current validators={2}  waiting for validators={3}".format(datetime.datetime.now(), remain_time, validators, count))
            if count== validators :
                print("validators ready")
                break
            time.sleep(10)


    def test_jailing(self) :
        print("test jailing")
        self.wait_for_ready(2)
        containers=self.get_containers()
        print(containers)
        assert "{}_chain1_1".format(CURRENT_HASH) in containers 
        print("wait for jailing")
        time.sleep(10)
        jailthis = containers["{}_chain1_1".format(CURRENT_HASH)]
        print("jail = " , jailthis)
        jailthis.kill()
        self.wait_for_ready(1)
        #jailed
        containers=self.get_containers()
        print(containers)
        assert "{}_chain1_1".format(CURRENT_HASH) not in containers
        print("jail test success")


    def test_unjailing(self) :
        initial_time=time.time() # in seconds
        print("test unjailing")
        self.wait_for_ready(1)

        MAX_TIME = 3600  
        while True:
            current_time= time.time()
            elasped_time= current_time - initial_time
            remain_time = MAX_TIME - elasped_time
            self.check_validators()
            if remain_time< 0 :
                assert False
            self.unjail("b",self.keyb, self.node1_address)
            state= self.get_staking_state("b",self.keyb, self.node1_address)
            print("state {}".format(state))
            punishment=state["punishment"] 
            print("{0}  remain time={1:.2f}  punishment {2}".format(datetime.datetime.now(), remain_time, punishment))
            if punishment is None :
                print("unjailed!!")
                break
            else :
                print("still jailed")
            time.sleep(10)
        print("unjail test success")

    ############################################################################3
    def main (self) :
        self.test_jailing()
        try :
            self.restore_wallets()
        except jsonrpcclient.exceptions.JsonRpcClientError as ex:
            print("wallet already exists={}".format(ex))
        self.keya=self.rpc.wallet.enckey("a")
        self.keyb=self.rpc.wallet.enckey("b") 
        self.keyc=self.rpc.wallet.enckey("c") 
        self.create_addresses()
        self.test_unjailing()

    def main2 (self) :
        try :
            self.restore_wallets()
        except jsonrpcclient.exceptions.JsonRpcClientError as ex:
            print("wallet already exists={}".format(ex))
        self.create_addresses()

    def read_info(self):
        print("read data")
        with open('info.json') as json_file:
            data = json.load(json_file)
        print(json.dumps(data,indent=4))
        self.node0_address= data["nodes"][0]["staking"][0]
        self.node1_address= data["nodes"][1]["staking"][0]
        self.node2_address= data["nodes"][2]["staking"][0]

        self.node0_mnemonics=data["nodes"][0]["mnemonic"]
        self.node1_mnemonics=data["nodes"][1]["mnemonic"]
        self.node2_mnemonics=data["nodes"][2]["mnemonic"]
        
    def display_info(self):
        print("jail test current hash={}".format(CURRENT_HASH))
        print("node0 staking= {}".format(self.node0_address))
        print("node1 staking= {}".format(self.node1_address))
        print("node2 staking= {}".format(self.node2_address))
        print("node0 mnemonics= {}".format(self.node0_mnemonics))
        print("node1 mnemonics= {}".format(self.node1_mnemonics))
        print("node2 mnemonics= {}".format(self.node2_mnemonics))


p = Program()
p.read_info()
p.display_info()
p.main()
