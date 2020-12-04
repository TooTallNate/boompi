// CSS
import styles from '@styles/connect-bluetooth.module.css';

interface ConnectBluetoothProps {
	name: string;
}

export default function ConnectBluetooth({ name }: ConnectBluetoothProps) {
	return (
		<div className={styles.bt}>
			<h3>To play music, connect your bluetooth device to:</h3>
			<h1>{name}</h1>
		</div>
	);
}
