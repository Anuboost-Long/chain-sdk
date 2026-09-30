def DisplayingMark():
    
    mark  = input("Please enter the mark: ")

    
    if (not mark.isdigit()):
        
        print("Numbers only !!!!")
        return 
    

    mark = float(mark)
    
    if(0 <= mark <= 100):
        
        print(f"Your mark is {mark}")
    
    else:
        
        print("Your mark is invalid")
        
DisplayingMark()



def ShowSaleOfDirt():
    
    delivery_fee = 20
    
    bags_amount = int(input("Please enter quantity of bags: "))
    
    price_per_bag = float(input("Please enter price per: "))

    sub_total = bags_amount * price_per_bag
    
    total = sub_total + delivery_fee

    print("\n" + "=" * 35)
    print("          DIRT SALE RECEIPT")
    print("=" * 35)
    print(f"Bags Amount        : {bags_amount}")
    print(f"Price Per Bag      : ${price_per_bag:.2f}")
    print("-" * 35)
    print(f"Subtotal           : ${sub_total:.2f}")
    print(f"Delivery Fee       : ${delivery_fee:.2f}")
    print("-" * 35)
    print(f"TOTAL              : ${total:.2f}")
    print("=" * 35)
    

ShowSaleOfDirt()